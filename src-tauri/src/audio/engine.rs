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
    AudioBackend, BackendError, EventSink, OutputFormat, RunningStream, StreamEvent,
};
use super::clip::Clip;
use super::mixer::{ControlError, Event, MixerHandle, Stats, mixer};

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
        gain: f32,
        reply: Sender<Result<(), EngineError>>,
    },
    StopAll {
        reply: Sender<Result<(), EngineError>>,
    },
    Retry,
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

    /// Starts `clip` at `gain` (1.0 is unity). The clip must match the current stream format.
    pub fn play(&self, clip: Arc<Clip>, gain: f32) -> Result<(), EngineError> {
        self.request(|reply| Message::Play { clip, gain, reply })
    }

    pub fn stop_all(&self) -> Result<(), EngineError> {
        self.request(|reply| Message::StopAll { reply })
    }

    /// Asks a `Failed` engine to try opening the stream again now.
    pub fn retry(&self) -> Result<(), EngineError> {
        self.sender
            .try_send(Message::Retry)
            .map_err(|_| EngineError::Busy)
    }

    fn request(
        &self,
        build: impl FnOnce(Sender<Result<(), EngineError>>) -> Message,
    ) -> Result<(), EngineError> {
        let (reply, answer) = channel();
        self.sender
            .try_send(build(reply))
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => EngineError::Busy,
                std::sync::mpsc::TrySendError::Disconnected(_) => EngineError::Stopped,
            })?;
        match answer.recv_timeout(REPLY_TIMEOUT) {
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

struct Session {
    // Held only so that dropping the session stops the stream.
    _stream: Box<dyn RunningStream>,
    handle: MixerHandle,
    format: OutputFormat,
}

struct Worker {
    backend: Box<dyn AudioBackend>,
    config: EngineConfig,
    shared: Arc<Shared>,
    sender: SyncSender<Message>,
    /// Identifies the current stream; bumped on every attempt to open one.
    generation: u64,
    session: Option<Session>,
    /// Failed opens in a row.
    attempt: u32,
    retry_at: Option<Instant>,
    next_poll: Instant,
    last_default: Option<String>,
}

impl Worker {
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
            attempt: 0,
            retry_at: None,
            next_poll: Instant::now(),
            last_default: None,
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
        }
        self.stop();
    }

    fn stop(mut self) {
        self.session = None;
        self.shared.set_mixer_stats(None);
        self.shared.set(EngineState::Stopped, None);
    }

    fn next_wakeup(&self) -> Instant {
        match self.retry_at {
            Some(retry_at) => retry_at.min(self.next_poll),
            None => self.next_poll,
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Play { clip, gain, reply } => {
                let _ = reply.send(self.play(clip, gain));
            }
            Message::StopAll { reply } => {
                let result = match self.session.as_mut() {
                    Some(session) => session.handle.stop_all().map_err(|_| EngineError::Busy),
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

    fn play(&mut self, clip: Arc<Clip>, gain: f32) -> Result<(), EngineError> {
        let session = self.session.as_mut().ok_or(EngineError::Unavailable)?;
        let matches_stream = clip.channels() == session.format.channels
            && clip.sample_rate() == session.format.sample_rate;
        if !matches_stream {
            return Err(EngineError::FormatMismatch);
        }
        session
            .handle
            .play(clip, gain)
            .map(|_| ())
            .map_err(|error| match error {
                ControlError::Busy => EngineError::Busy,
                ControlError::ChannelMismatch => EngineError::FormatMismatch,
            })
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

    fn tick(&mut self) {
        let now = Instant::now();
        if let Some(session) = self.session.as_mut() {
            for event in session.handle.drain() {
                if matches!(event, Event::PlayRejected(_)) {
                    self.shared.rejected_plays.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        if self.retry_at.is_some_and(|at| at <= now) {
            self.open();
        }
        if now >= self.next_poll {
            self.poll_default_output();
            self.next_poll = Instant::now() + self.config.poll_interval;
        }
    }

    /// Streams do not reliably announce a change of the system default output on every platform, so the identity is checked
    /// periodically as well.
    fn poll_default_output(&mut self) {
        let current = self.backend.default_output_id();
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
            let (mut mixer, handle) = mixer(format.channels);
            handle_slot = Some(handle);
            Box::new(move |out: &mut [f32]| mixer.render(out)) as super::backend::RenderFn
        };
        match self.backend.open_default_output(&mut make_render, sink) {
            Ok((stream, format)) => {
                let Some(handle) = handle_slot else {
                    // A backend that never asked for the callback did not start a usable stream.
                    return self.open_failed(BackendError::Other);
                };
                let stats = Arc::clone(handle.stats());
                self.session = Some(Session {
                    _stream: stream,
                    handle,
                    format,
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
