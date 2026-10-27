//! The audio engine: one thread that owns the output stream and keeps it alive (ADR 0006).
//!
//! The engine thread opens the default output, runs the device state machine
//! (`Starting`, `Running`, `Recovering`, `Failed`, `Stopped`), drains the mixer's retired clips and events, and rebuilds the
//! stream when the device is lost, the stream is invalidated, or the system default output changes. Decoding never happens
//! here: callers hand over ready clips. Every wait is bounded and every queue is bounded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError, channel, sync_channel,
};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::backend::{
    AudioBackend, BackendError, EventSink, OutputFormat, RouteCounters, RouteDevices, RouteMetrics,
    RunningStream, StreamEvent,
};
use super::clip::Clip;
use super::mixer::{ControlError, Event, MixerHandle, Stats, VoiceId, mixer};

const MESSAGE_CAPACITY: usize = 64;
/// How long a caller waits for the engine thread to answer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    NoDevice,
    PermissionDenied,
    Unsupported,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Starting,
    Running,
    /// The stream was lost or could not be opened; retrying with backoff. `attempt` counts failed attempts so far.
    Recovering {
        attempt: u32,
    },
    /// Retrying gave up or the failure cannot be retried. `retry` (or a newly appearing default device) tries again.
    Failed(FailureReason),
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSnapshot {
    pub state: EngineState,
    /// The format of the open stream, if any. Clips must be decoded to this format.
    pub format: Option<OutputFormat>,
    pub xruns: u64,
    pub device_losses: u64,
    pub opens: u64,
    pub rejected_plays: u64,
    /// Audio callbacks run by the current stream; proves that the device is pulling audio.
    pub callbacks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteState {
    Stopped,
    Running,
    Failed(FailureReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteSnapshot {
    pub state: RouteState,
    pub format: Option<OutputFormat>,
    pub counters: Option<RouteCounters>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineError {
    /// There is no running stream right now (starting, recovering or failed).
    Unavailable,
    /// The clip was decoded for a different format than the stream now uses; decode it again.
    FormatMismatch,
    /// A queue is full or the engine did not answer in time.
    Busy,
    /// The engine has been shut down.
    Stopped,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// How often the default output device is checked and finished clips are released.
    pub poll_interval: Duration,
    /// Waits between failed attempts to open the stream; once exhausted the engine is `Failed`.
    pub retry_delays: Vec<Duration>,
}

impl EngineConfig {
    pub fn production() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            retry_delays: [250, 500, 1000, 2000, 4000]
                .into_iter()
                .map(Duration::from_millis)
                .collect(),
        }
    }
}

enum Message {
    Play {
        clip: Arc<Clip>,
        route_clip: Option<Arc<Clip>>,
        gain: f32,
        tag: Option<VoiceTag>,
        reply: Sender<Result<(), EngineError>>,
    },
    StopAll {
        reply: Sender<Result<(), EngineError>>,
    },
    StopTag {
        tag: VoiceTag,
        reply: Sender<Result<(), EngineError>>,
    },
    Playing {
        tag: VoiceTag,
        reply: Sender<Result<bool, EngineError>>,
    },
    SetMasterGain {
        gain: f32,
        reply: Sender<Result<(), EngineError>>,
    },
    PlayingTags {
        reply: Sender<Result<Vec<VoiceTag>, EngineError>>,
    },
    SetPlayingListener {
        listener: PlayingListener,
        reply: Sender<Result<(), EngineError>>,
    },
    Retry,
    StartRoute {
        devices: RouteDevices,
        reply: Sender<Result<(), EngineError>>,
    },
    StopRoute {
        reply: Sender<Result<(), EngineError>>,
    },
    RoutePlay {
        clip: Arc<Clip>,
        gain: f32,
        tag: VoiceTag,
        reply: Sender<Result<(), EngineError>>,
    },
    SetLocalOutput {
        id: Option<String>,
        reply: Sender<Result<(), EngineError>>,
    },
    RouteStream {
        generation: u64,
        event: StreamEvent,
    },
    /// An event from the audio API. `generation` identifies the stream that raised it; events of a stream that has been
    /// replaced are stale and ignored.
    Stream {
        generation: u64,
        event: StreamEvent,
    },
    Shutdown,
}

struct Shared {
    snapshot: RwLock<(EngineState, Option<OutputFormat>)>,
    xruns: AtomicU64,
    device_losses: AtomicU64,
    opens: AtomicU64,
    rejected_plays: AtomicU64,
    /// The current stream's mixer counters, read live so a snapshot is never stale.
    mixer_stats: RwLock<Option<Arc<Stats>>>,
    route: RwLock<(RouteState, Option<OutputFormat>, Option<Arc<RouteMetrics>>)>,
}

impl Shared {
    fn callbacks(&self) -> u64 {
        let stats = self
            .mixer_stats
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stats
            .as_ref()
            .map_or(0, |stats| stats.callbacks.load(Ordering::Relaxed))
    }

    fn set_mixer_stats(&self, stats: Option<Arc<Stats>>) {
        *self
            .mixer_stats
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = stats;
    }

    fn set(&self, state: EngineState, format: Option<OutputFormat>) {
        *self
            .snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = (state, format);
    }
}

/// The handle used by the rest of the application. Cheap to share by reference.
pub struct AudioEngine {
    sender: SyncSender<Message>,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl AudioEngine {
    /// Starts the engine thread. `make_backend` runs on that thread, so the backend and its streams never cross threads.
    pub fn start(
        config: EngineConfig,
        make_backend: impl FnOnce() -> Box<dyn AudioBackend> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = sync_channel(MESSAGE_CAPACITY);
        let shared = Arc::new(Shared {
            snapshot: RwLock::new((EngineState::Starting, None)),
            xruns: AtomicU64::new(0),
            device_losses: AtomicU64::new(0),
            opens: AtomicU64::new(0),
            rejected_plays: AtomicU64::new(0),
            mixer_stats: RwLock::new(None),
            route: RwLock::new((RouteState::Stopped, None, None)),
        });
        let worker_sender = sender.clone();
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("screampad-audio".to_owned())
            .spawn(move || {
                Worker::new(make_backend(), config, worker_shared, worker_sender).run(&receiver);
            })?;
        Ok(Self {
            sender,
            shared,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn snapshot(&self) -> EngineSnapshot {
        let (state, format) = *self
            .shared
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        EngineSnapshot {
            state,
            format,
            xruns: self.shared.xruns.load(Ordering::Relaxed),
            device_losses: self.shared.device_losses.load(Ordering::Relaxed),
            opens: self.shared.opens.load(Ordering::Relaxed),
            rejected_plays: self.shared.rejected_plays.load(Ordering::Relaxed),
            callbacks: self.shared.callbacks(),
        }
    }

    pub fn route_snapshot(&self) -> RouteSnapshot {
        let route = self
            .shared
            .route
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        RouteSnapshot {
            state: route.0,
            format: route.1,
            counters: route.2.as_ref().map(|metrics| metrics.snapshot()),
        }
    }

    pub fn record_route_clip_failure(&self) {
        let route = self
            .shared
            .route
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(metrics) = &route.2 {
            metrics.clip_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn start_route(&self, devices: RouteDevices) -> Result<(), EngineError> {
        self.request_timeout(Duration::from_secs(30), |reply| Message::StartRoute {
            devices,
            reply,
        })
    }

    pub fn stop_route(&self) -> Result<(), EngineError> {
        self.request(|reply| Message::StopRoute { reply })
    }

    pub fn route_play_tagged(
        &self,
        clip: Arc<Clip>,
        gain: f32,
        tag: VoiceTag,
    ) -> Result<(), EngineError> {
        self.request(|reply| Message::RoutePlay {
            clip,
            gain,
            tag,
            reply,
        })
    }

    pub fn set_local_output(&self, id: Option<String>) -> Result<(), EngineError> {
        self.request(|reply| Message::SetLocalOutput { id, reply })
    }

    /// Starts `clip` at `gain` (1.0 is unity). The clip must match the current stream format.
    pub fn play(&self, clip: Arc<Clip>, gain: f32) -> Result<(), EngineError> {
        self.request(|reply| Message::Play {
            clip,
            route_clip: None,
            gain,
            tag: None,
            reply,
        })
    }

    /// Like `play`, and remembers that the voice belongs to `tag` (the caller's own identifier, such as a sound id), so
    /// that `playing` and `stop_tag` can find it.
    pub fn play_tagged(
        &self,
        clip: Arc<Clip>,
        gain: f32,
        tag: VoiceTag,
    ) -> Result<(), EngineError> {
        self.request(|reply| Message::Play {
            clip,
            route_clip: None,
            gain,
            tag: Some(tag),
            reply,
        })
    }

    /// Starts the local and optional route clip under one control-thread decision.
    pub fn play_tagged_routed(
        &self,
        clip: Arc<Clip>,
        route_clip: Option<Arc<Clip>>,
        gain: f32,
        tag: VoiceTag,
    ) -> Result<(), EngineError> {
        self.request(|reply| Message::Play {
            clip,
            route_clip,
            gain,
            tag: Some(tag),
            reply,
        })
    }

    pub fn stop_all(&self) -> Result<(), EngineError> {
        self.request(|reply| Message::StopAll { reply })
    }

    /// Stops every voice started with `tag`.
    pub fn stop_tag(&self, tag: VoiceTag) -> Result<(), EngineError> {
        self.request(|reply| Message::StopTag { tag, reply })
    }

    /// Whether a voice started with `tag` is still sounding. A voice that has been asked to stop no longer counts.
    pub fn playing(&self, tag: VoiceTag) -> Result<bool, EngineError> {
        self.request(|reply| Message::Playing { tag, reply })
    }

    /// The tags (for example sound ids) that have a voice sounding right now, sorted, without duplicates. A voice that has
    /// been asked to stop no longer counts.
    pub fn playing_tags(&self) -> Result<Vec<VoiceTag>, EngineError> {
        self.request(|reply| Message::PlayingTags { reply })
    }

    /// Registers the function the engine calls whenever the set of playing tags changes (after a play, a stop, a voice that
    /// ended by itself, or a rebuilt stream). It runs on the engine thread: it must return quickly and must not call back
    /// into the engine. Replaces an earlier listener.
    pub fn on_playing_changed(
        &self,
        listener: impl Fn(&[VoiceTag]) + Send + Sync + 'static,
    ) -> Result<(), EngineError> {
        let listener: PlayingListener = Arc::new(listener);
        self.request(|reply| Message::SetPlayingListener { listener, reply })
    }

    /// Sets the linear master gain (1.0 is unity). It is kept across stream rebuilds, so a device change does not reset it.
    pub fn set_master_gain(&self, gain: f32) -> Result<(), EngineError> {
        self.request(|reply| Message::SetMasterGain { gain, reply })
    }

    /// Asks a `Failed` engine to try opening the stream again now.
    pub fn retry(&self) -> Result<(), EngineError> {
        self.sender
            .try_send(Message::Retry)
            .map_err(|_| EngineError::Busy)
    }

    fn request<T>(
        &self,
        build: impl FnOnce(Sender<Result<T, EngineError>>) -> Message,
    ) -> Result<T, EngineError> {
        self.request_timeout(REPLY_TIMEOUT, build)
    }

    fn request_timeout<T>(
        &self,
        timeout: Duration,
        build: impl FnOnce(Sender<Result<T, EngineError>>) -> Message,
    ) -> Result<T, EngineError> {
        let (reply, answer) = channel();
        self.sender
            .try_send(build(reply))
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => EngineError::Busy,
                std::sync::mpsc::TrySendError::Disconnected(_) => EngineError::Stopped,
            })?;
        match answer.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(EngineError::Busy),
            Err(RecvTimeoutError::Disconnected) => Err(EngineError::Stopped),
        }
    }

    /// Stops the stream and the thread. Safe to call more than once; also done on drop.
    pub fn shutdown(&self) {
        let _ = self.sender.send(Message::Shutdown);
        let handle = self
            .thread
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The caller's identifier for a group of voices (a sound id, for example).
pub type VoiceTag = u64;

type PlayingListener = Arc<dyn Fn(&[VoiceTag]) + Send + Sync>;

/// How often the engine looks for voices that have ended while some are sounding, so the UI follows a sound that finishes
/// by itself within about this long.
const VOICE_WATCH: Duration = Duration::from_millis(50);

/// A voice the engine has handed to the mixer and not yet seen finish.
struct TrackedVoice {
    id: VoiceId,
    tag: Option<VoiceTag>,
    /// Asked to stop; it may still sound for a few milliseconds while it fades out.
    stopping: bool,
}

struct Session {
    // Held only so that dropping the session stops the stream.
    _stream: Box<dyn RunningStream>,
    handle: MixerHandle,
    format: OutputFormat,
    voices: Vec<TrackedVoice>,
}

struct Worker {
    backend: Box<dyn AudioBackend>,
    config: EngineConfig,
    shared: Arc<Shared>,
    sender: SyncSender<Message>,
    /// Identifies the current stream; bumped on every attempt to open one.
    generation: u64,
    session: Option<Session>,
    route: Option<Session>,
    route_devices: Option<RouteDevices>,
    route_generation: u64,
    missing_route_device_polls: u8,
    selected_output: Option<String>,
    /// Failed opens in a row.
    attempt: u32,
    retry_at: Option<Instant>,
    next_poll: Instant,
    last_default: Option<String>,
    /// Linear master gain, applied to every stream that is opened.
    master_gain: f32,
    playing_listener: Option<PlayingListener>,
    /// The set last reported to the listener.
    reported_playing: Vec<VoiceTag>,
}

impl Worker {
    fn set_route_status(
        &self,
        state: RouteState,
        format: Option<OutputFormat>,
        metrics: Option<Arc<RouteMetrics>>,
    ) {
        *self
            .shared
            .route
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = (state, format, metrics);
    }

    fn start_route(&mut self, devices: RouteDevices) -> Result<(), EngineError> {
        if self.route.is_some() && self.route_devices.as_ref() == Some(&devices) {
            return Ok(());
        }
        self.stop_route();
        let local_id = self
            .selected_output
            .clone()
            .or_else(|| self.backend.default_output_id());
        if devices.microphone_id == devices.virtual_output_id
            || local_id.as_deref() == Some(devices.virtual_output_id.as_str())
        {
            self.set_route_status(RouteState::Failed(FailureReason::Unsupported), None, None);
            return Err(EngineError::Unavailable);
        }
        self.route_generation += 1;
        let generation = self.route_generation;
        let sender = self.sender.clone();
        let events: EventSink = Arc::new(move |event| {
            let _ = sender.try_send(Message::RouteStream { generation, event });
        });
        let mut handle_slot = None;
        let mut make_render = |format: OutputFormat| {
            let (mut mixer, handle) = mixer(format.channels, format.sample_rate);
            handle_slot = Some(handle);
            Box::new(move |out: &mut [f32]| mixer.render(out)) as super::backend::RenderFn
        };
        match self.backend.open_route(&devices, &mut make_render, events) {
            Ok((stream, format, metrics)) => {
                let Some(handle) = handle_slot else {
                    self.set_route_status(RouteState::Failed(FailureReason::Other), None, None);
                    return Err(EngineError::Unavailable);
                };
                self.route = Some(Session {
                    _stream: stream,
                    handle,
                    format,
                    voices: Vec::new(),
                });
                self.route_devices = Some(devices);
                self.missing_route_device_polls = 0;
                self.set_route_status(RouteState::Running, Some(format), Some(metrics));
                Ok(())
            }
            Err(error) => {
                let reason = match error {
                    BackendError::NoDevice | BackendError::Busy => FailureReason::NoDevice,
                    BackendError::PermissionDenied => FailureReason::PermissionDenied,
                    BackendError::Unsupported => FailureReason::Unsupported,
                    BackendError::Other => FailureReason::Other,
                };
                self.set_route_status(RouteState::Failed(reason), None, None);
                Err(EngineError::Unavailable)
            }
        }
    }

    fn stop_route(&mut self) {
        self.route_generation += 1;
        self.route = None;
        self.route_devices = None;
        self.missing_route_device_polls = 0;
        self.set_route_status(RouteState::Stopped, None, None);
    }

    fn fail_route(&mut self, reason: FailureReason) {
        if self.route.take().is_some() {
            self.route_generation += 1;
            self.route_devices = None;
            // Retain the final counters so a device failure can be diagnosed without
            // keeping either stream or the microphone capture alive.
            let metrics = self
                .shared
                .route
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .2
                .clone();
            self.set_route_status(RouteState::Failed(reason), None, metrics);
        }
    }

    fn route_play(&mut self, clip: Arc<Clip>, gain: f32, tag: VoiceTag) -> Result<(), EngineError> {
        self.sync_route_voices();
        let route = self.route.as_mut().ok_or(EngineError::Unavailable)?;
        if clip.channels() != route.format.channels
            || clip.sample_rate() != route.format.sample_rate
        {
            return Err(EngineError::FormatMismatch);
        }
        // Keep the clip below speech level in chat processing; the user's stored gain still scales it.
        let id = route
            .handle
            .play(clip, gain * 0.2)
            .map_err(|error| match error {
                ControlError::Busy => EngineError::Busy,
                ControlError::ChannelMismatch => EngineError::FormatMismatch,
            })?;
        route.voices.push(TrackedVoice {
            id,
            tag: Some(tag),
            stopping: false,
        });
        Ok(())
    }

    fn stop_route_voices(&mut self, tag: Option<VoiceTag>) {
        self.sync_route_voices();
        let Some(route) = self.route.as_mut() else {
            return;
        };
        match tag {
            None => {
                let _ = route.handle.stop_all();
                for voice in &mut route.voices {
                    voice.stopping = true;
                }
            }
            Some(tag) => {
                for voice in &mut route.voices {
                    if voice.tag == Some(tag)
                        && !voice.stopping
                        && route.handle.stop(voice.id).is_ok()
                    {
                        voice.stopping = true;
                    }
                }
            }
        }
    }

    fn sync_route_voices(&mut self) {
        if let Some(route) = self.route.as_mut() {
            for event in route.handle.drain() {
                match event {
                    Event::VoiceFinished(id) | Event::PlayRejected(id) => {
                        route.voices.retain(|voice| voice.id != id)
                    }
                }
            }
        }
    }

    fn new(
        backend: Box<dyn AudioBackend>,
        config: EngineConfig,
        shared: Arc<Shared>,
        sender: SyncSender<Message>,
    ) -> Self {
        Self {
            backend,
            config,
            shared,
            sender,
            generation: 0,
            session: None,
            route: None,
            route_devices: None,
            route_generation: 0,
            missing_route_device_polls: 0,
            selected_output: None,
            attempt: 0,
            retry_at: None,
            next_poll: Instant::now(),
            last_default: None,
            master_gain: 1.0,
            playing_listener: None,
            reported_playing: Vec::new(),
        }
    }

    fn run(mut self, receiver: &Receiver<Message>) {
        self.open();
        loop {
            let wait = self.next_wakeup().saturating_duration_since(Instant::now());
            match receiver.recv_timeout(wait) {
                Ok(Message::Shutdown) => break,
                Ok(message) => self.handle(message),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            // Handle everything already queued before doing periodic work.
            loop {
                match receiver.try_recv() {
                    Ok(Message::Shutdown) => return self.stop(),
                    Ok(message) => self.handle(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return self.stop(),
                }
            }
            self.tick();
            self.publish_playing(false);
        }
        self.stop();
    }

    fn stop(mut self) {
        self.stop_route();
        self.session = None;
        self.shared.set_mixer_stats(None);
        self.shared.set(EngineState::Stopped, None);
    }

    fn next_wakeup(&self) -> Instant {
        let base = match self.retry_at {
            Some(retry_at) => retry_at.min(self.next_poll),
            None => self.next_poll,
        };
        if self
            .session
            .as_ref()
            .is_some_and(|session| !session.voices.is_empty())
        {
            base.min(Instant::now() + VOICE_WATCH)
        } else {
            base
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Play {
                clip,
                route_clip,
                gain,
                tag,
                reply,
            } => {
                let result = self.play(clip, gain, tag);
                let route_failed = result.is_ok()
                    && match (route_clip, tag) {
                        (Some(route_clip), Some(tag)) => {
                            self.route_play(route_clip, gain, tag).is_err()
                        }
                        _ => false,
                    };
                if route_failed
                    && let Some(metrics) = &self
                        .shared
                        .route
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .2
                {
                    metrics.clip_errors.fetch_add(1, Ordering::Relaxed);
                }
                let _ = reply.send(result);
            }
            Message::StopAll { reply } => {
                let result = match self.session.as_mut() {
                    Some(session) => {
                        for voice in &mut session.voices {
                            voice.stopping = true;
                        }
                        session.handle.stop_all().map_err(|_| EngineError::Busy)
                    }
                    None => Ok(()),
                };
                self.stop_route_voices(None);
                let _ = reply.send(result);
            }
            Message::StopTag { tag, reply } => {
                self.stop_route_voices(Some(tag));
                let _ = reply.send(self.stop_tag(tag));
            }
            Message::Playing { tag, reply } => {
                let _ = reply.send(Ok(self.playing(tag)));
            }
            Message::PlayingTags { reply } => {
                self.sync_voices();
                let _ = reply.send(Ok(self.playing_now()));
            }
            Message::SetPlayingListener { listener, reply } => {
                self.playing_listener = Some(listener);
                // The new listener learns the current state, even if it is empty.
                self.reported_playing = Vec::new();
                self.publish_playing(true);
                let _ = reply.send(Ok(()));
            }
            Message::SetMasterGain { gain, reply } => {
                self.master_gain = gain;
                let result = match self.session.as_mut() {
                    Some(session) => session
                        .handle
                        .set_master_gain(gain)
                        .map_err(|_| EngineError::Busy),
                    None => Ok(()),
                };
                let _ = reply.send(result);
            }
            Message::Retry => {
                if matches!(self.state(), EngineState::Failed(_)) {
                    self.attempt = 0;
                    self.open();
                }
            }
            Message::StartRoute { devices, reply } => {
                let _ = reply.send(self.start_route(devices));
            }
            Message::StopRoute { reply } => {
                self.stop_route();
                let _ = reply.send(Ok(()));
            }
            Message::RoutePlay {
                clip,
                gain,
                tag,
                reply,
            } => {
                let _ = reply.send(self.route_play(clip, gain, tag));
            }
            Message::SetLocalOutput { id, reply } => {
                let result =
                    if self.route_devices.as_ref().is_some_and(|route| {
                        id.as_deref() == Some(route.virtual_output_id.as_str())
                    }) {
                        Err(EngineError::Unavailable)
                    } else {
                        let previous = self.selected_output.clone();
                        self.selected_output = id;
                        self.session = None;
                        self.shared.set_mixer_stats(None);
                        self.open();
                        if matches!(self.state(), EngineState::Running) {
                            Ok(())
                        } else {
                            self.selected_output = previous;
                            self.session = None;
                            self.shared.set_mixer_stats(None);
                            self.open();
                            Err(EngineError::Unavailable)
                        }
                    };
                let _ = reply.send(result);
            }
            Message::RouteStream { generation, event } => {
                if generation == self.route_generation && !matches!(event, StreamEvent::Xrun) {
                    self.fail_route(FailureReason::NoDevice);
                }
            }
            Message::Stream { generation, event } => {
                if generation == self.generation {
                    self.stream_event(event);
                }
            }
            Message::Shutdown => {}
        }
    }

    fn state(&self) -> EngineState {
        self.shared
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0
    }

    fn play(
        &mut self,
        clip: Arc<Clip>,
        gain: f32,
        tag: Option<VoiceTag>,
    ) -> Result<(), EngineError> {
        self.sync_voices();
        let session = self.session.as_mut().ok_or(EngineError::Unavailable)?;
        let matches_stream = clip.channels() == session.format.channels
            && clip.sample_rate() == session.format.sample_rate;
        if !matches_stream {
            return Err(EngineError::FormatMismatch);
        }
        let id = session
            .handle
            .play(clip, gain)
            .map_err(|error| match error {
                ControlError::Busy => EngineError::Busy,
                ControlError::ChannelMismatch => EngineError::FormatMismatch,
            })?;
        session.voices.push(TrackedVoice {
            id,
            tag,
            stopping: false,
        });
        Ok(())
    }

    fn stop_tag(&mut self, tag: VoiceTag) -> Result<(), EngineError> {
        self.sync_voices();
        let Some(session) = self.session.as_mut() else {
            return Ok(());
        };
        let mut busy = false;
        for voice in &mut session.voices {
            if voice.tag == Some(tag) && !voice.stopping {
                match session.handle.stop(voice.id) {
                    Ok(()) => voice.stopping = true,
                    Err(_) => busy = true,
                }
            }
        }
        if busy { Err(EngineError::Busy) } else { Ok(()) }
    }

    fn playing(&mut self, tag: VoiceTag) -> bool {
        self.sync_voices();
        self.session.as_ref().is_some_and(|session| {
            session
                .voices
                .iter()
                .any(|voice| voice.tag == Some(tag) && !voice.stopping)
        })
    }

    /// Reads what the audio callback reported since the last call: frees finished clips, forgets finished voices and counts
    /// rejected plays.
    fn sync_voices(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        for event in session.handle.drain() {
            match event {
                Event::VoiceFinished(id) => session.voices.retain(|voice| voice.id != id),
                Event::PlayRejected(id) => {
                    session.voices.retain(|voice| voice.id != id);
                    self.shared.rejected_plays.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn stream_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::Xrun => {
                self.shared.xruns.fetch_add(1, Ordering::Relaxed);
            }
            StreamEvent::DefaultChanged => self.next_poll = Instant::now(),
            StreamEvent::DeviceLost | StreamEvent::Invalidated | StreamEvent::Failed => {
                self.lose_session()
            }
        }
    }

    /// Drops the stream (and its voices) and schedules an immediate reopen. Events that arrive while already recovering do
    /// nothing, so a burst of errors causes one rebuild.
    fn lose_session(&mut self) {
        if self.session.take().is_none() {
            return;
        }
        self.shared.device_losses.fetch_add(1, Ordering::Relaxed);
        self.shared.set_mixer_stats(None);
        self.attempt = 0;
        self.retry_at = Some(Instant::now());
        self.shared
            .set(EngineState::Recovering { attempt: 0 }, None);
    }

    /// The sorted, deduplicated tags that have a voice that is not stopping.
    fn playing_now(&self) -> Vec<VoiceTag> {
        let mut tags: Vec<VoiceTag> = self
            .session
            .as_ref()
            .map(|session| {
                session
                    .voices
                    .iter()
                    .filter(|voice| !voice.stopping)
                    .filter_map(|voice| voice.tag)
                    .collect()
            })
            .unwrap_or_default();
        tags.sort_unstable();
        tags.dedup();
        tags
    }

    /// Tells the listener about the playing set when it differs from what it was last told (or always, with `force`).
    fn publish_playing(&mut self, force: bool) {
        let now = self.playing_now();
        if !force && now == self.reported_playing {
            return;
        }
        self.reported_playing = now;
        if let Some(listener) = &self.playing_listener {
            listener(&self.reported_playing);
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        self.sync_voices();
        let route_failed = {
            let shared = self
                .shared
                .route
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            shared
                .2
                .as_ref()
                .is_some_and(|stats| stats.failed.load(Ordering::Acquire))
        };
        if route_failed {
            self.fail_route(FailureReason::NoDevice);
        }
        self.sync_route_voices();
        if self.retry_at.is_some_and(|at| at <= now) {
            self.open();
        }
        if now >= self.next_poll {
            self.poll_default_output();
            // CoreAudio enumeration can briefly omit a healthy device. Require several
            // consecutive misses; stream errors still stop routing immediately.
            if let Some(devices) = self.route_devices.as_ref() {
                if self.backend.route_devices_available(devices) {
                    self.missing_route_device_polls = 0;
                } else {
                    self.missing_route_device_polls += 1;
                    if self.missing_route_device_polls >= 3 {
                        self.fail_route(FailureReason::NoDevice);
                    }
                }
            }
            self.next_poll = Instant::now() + self.config.poll_interval;
        }
    }

    /// Streams do not reliably announce a change of the system default output on every platform, so the identity is checked
    /// periodically as well.
    fn poll_default_output(&mut self) {
        let current = self.backend.default_output_id();
        if self
            .route_devices
            .as_ref()
            .is_some_and(|route| current.as_deref() == Some(route.virtual_output_id.as_str()))
            && self.selected_output.is_none()
        {
            self.fail_route(FailureReason::Unsupported);
        }
        if self.selected_output.is_some() {
            return;
        }
        let changed = current != self.last_default;
        match (&self.session, self.state()) {
            (Some(_), _) if changed => {
                // The default moved (or vanished): rebuild on the new one.
                self.last_default = current;
                self.lose_session();
            }
            (None, EngineState::Failed(_)) if changed && current.is_some() => {
                // A device appeared while there was none: try again.
                self.last_default = current;
                self.attempt = 0;
                self.open();
            }
            _ => self.last_default = current,
        }
    }

    fn open(&mut self) {
        self.retry_at = None;
        self.generation += 1;
        let generation = self.generation;
        let sender = self.sender.clone();
        let sink: EventSink = Arc::new(move |event| {
            // Called from OS audio threads. A full queue drops the hint; the periodic check still catches the change.
            let _ = sender.try_send(Message::Stream { generation, event });
        });
        let mut handle_slot: Option<MixerHandle> = None;
        let mut make_render = |format: OutputFormat| {
            let (mut mixer, handle) = mixer(format.channels, format.sample_rate);
            handle_slot = Some(handle);
            Box::new(move |out: &mut [f32]| mixer.render(out)) as super::backend::RenderFn
        };
        let opened = match &self.selected_output {
            Some(id) => self
                .backend
                .open_selected_output(id, &mut make_render, sink),
            None => self.backend.open_default_output(&mut make_render, sink),
        };
        match opened {
            Ok((stream, format)) => {
                let Some(handle) = handle_slot else {
                    // A backend that never asked for the callback did not start a usable stream.
                    return self.open_failed(BackendError::Other);
                };
                let stats = Arc::clone(handle.stats());
                let mut handle = handle;
                // A new stream starts at unity; the master gain the user chose must survive a device change. The queue is
                // empty here, so this cannot be refused.
                let _ = handle.set_master_gain(self.master_gain);
                self.session = Some(Session {
                    _stream: stream,
                    handle,
                    format,
                    voices: Vec::new(),
                });
                self.attempt = 0;
                self.last_default = self.backend.default_output_id();
                self.shared.opens.fetch_add(1, Ordering::Relaxed);
                self.shared.set_mixer_stats(Some(stats));
                self.shared.set(EngineState::Running, Some(format));
            }
            Err(error) => self.open_failed(error),
        }
    }

    fn open_failed(&mut self, error: BackendError) {
        self.session = None;
        self.shared.set_mixer_stats(None);
        // Remember which device this failure was about, so that only a later *change* of the default output counts as a
        // reason to try again (otherwise the first periodic check would retry a failure that cannot be retried).
        self.last_default = self.backend.default_output_id();
        let delays = &self.config.retry_delays;
        let index = self.attempt as usize;
        if error.is_retryable() && index < delays.len() {
            self.attempt += 1;
            self.retry_at = Some(Instant::now() + delays[index]);
            self.shared.set(
                EngineState::Recovering {
                    attempt: self.attempt,
                },
                None,
            );
        } else {
            let reason = match error {
                BackendError::NoDevice | BackendError::Busy => FailureReason::NoDevice,
                BackendError::PermissionDenied => FailureReason::PermissionDenied,
                BackendError::Unsupported => FailureReason::Unsupported,
                BackendError::Other => FailureReason::Other,
            };
            self.shared.set(EngineState::Failed(reason), None);
        }
    }
}
