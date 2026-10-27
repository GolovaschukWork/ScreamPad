//! The port between the audio engine and an audio API. The engine never names `cpal`; tests use a fake backend.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Called from the audio thread with an interleaved buffer to fill. Must not allocate, block or log.
pub type RenderFn = Box<dyn FnMut(&mut [f32]) + Send>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDevices {
    pub microphone_id: String,
    pub virtual_output_id: String,
}

#[derive(Default)]
pub struct RouteMetrics {
    pub input_rate: u32,
    pub output_rate: u32,
    pub queue_capacity: u32,
    pub input_buffer_frames: AtomicU64,
    pub output_buffer_frames: AtomicU64,
    pub failed: AtomicBool,
    pub captured: AtomicU64,
    pub rendered: AtomicU64,
    pub occupancy: AtomicU64,
    pub overflow: AtomicU64,
    pub underflow: AtomicU64,
    pub discarded: AtomicU64,
    pub max_occupancy: AtomicU64,
    pub clipped: AtomicU64,
    pub xruns: AtomicU64,
    pub clip_errors: AtomicU64,
    pub mic_energy: AtomicU64,
    pub clip_energy: AtomicU64,
    pub output_energy: AtomicU64,
    pub output_peak: AtomicU64,
    /// Recent callback RMS levels in parts per million of full scale.
    pub mic_level_ppm: AtomicU64,
    pub input_level_ppm: AtomicU64,
    pub output_level_ppm: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteCounters {
    pub input_rate: u32,
    pub output_rate: u32,
    pub queue_capacity: u32,
    pub input_buffer_frames: u32,
    pub output_buffer_frames: u32,
    pub captured: u64,
    pub rendered: u64,
    pub occupancy: u64,
    pub overflow: u64,
    pub underflow: u64,
    pub discarded: u64,
    pub max_occupancy: u64,
    pub clipped: u64,
    pub xruns: u64,
    pub clip_errors: u64,
    pub mic_energy: u64,
    pub clip_energy: u64,
    pub output_energy: u64,
    pub output_peak: u64,
    pub mic_level_ppm: u32,
    pub input_level_ppm: u32,
    pub output_level_ppm: u32,
}

pub type OpenRouteResult =
    Result<(Box<dyn RunningStream>, OutputFormat, Arc<RouteMetrics>), BackendError>;

impl RouteMetrics {
    pub fn snapshot(&self) -> RouteCounters {
        RouteCounters {
            input_rate: self.input_rate,
            output_rate: self.output_rate,
            queue_capacity: self.queue_capacity,
            input_buffer_frames: u32::try_from(self.input_buffer_frames.load(Ordering::Relaxed))
                .unwrap_or(u32::MAX),
            output_buffer_frames: u32::try_from(self.output_buffer_frames.load(Ordering::Relaxed))
                .unwrap_or(u32::MAX),
            captured: self.captured.load(Ordering::Relaxed),
            rendered: self.rendered.load(Ordering::Relaxed),
            occupancy: self.occupancy.load(Ordering::Relaxed),
            overflow: self.overflow.load(Ordering::Relaxed),
            underflow: self.underflow.load(Ordering::Relaxed),
            discarded: self.discarded.load(Ordering::Relaxed),
            max_occupancy: self.max_occupancy.load(Ordering::Relaxed),
            clipped: self.clipped.load(Ordering::Relaxed),
            xruns: self.xruns.load(Ordering::Relaxed),
            clip_errors: self.clip_errors.load(Ordering::Relaxed),
            mic_energy: self.mic_energy.load(Ordering::Relaxed),
            clip_energy: self.clip_energy.load(Ordering::Relaxed),
            output_energy: self.output_energy.load(Ordering::Relaxed),
            output_peak: self.output_peak.load(Ordering::Relaxed),
            mic_level_ppm: u32::try_from(self.mic_level_ppm.load(Ordering::Relaxed))
                .unwrap_or(u32::MAX),
            input_level_ppm: u32::try_from(self.input_level_ppm.load(Ordering::Relaxed))
                .unwrap_or(u32::MAX),
            output_level_ppm: u32::try_from(self.output_level_ppm.load(Ordering::Relaxed))
                .unwrap_or(u32::MAX),
        }
    }
}

/// The format the output device runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// What the audio API reports about a running stream. Sent from an OS thread, so a sink must be cheap and non-blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEvent {
    /// The device went away.
    DeviceLost,
    /// The stream configuration is no longer valid and the stream must be rebuilt.
    Invalidated,
    /// The system default output may have changed; the engine checks it right away.
    DefaultChanged,
    /// A buffer underrun or overrun. Counted, never an error shown to the user.
    Xrun,
    /// Any other stream error; treated like a lost device.
    Failed,
}

pub type EventSink = Arc<dyn Fn(StreamEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendError {
    /// There is no output device (none connected, or the audio host is unavailable).
    NoDevice,
    /// The device is busy or a resource limit was hit; trying again later may work.
    Busy,
    /// The device cannot run a channel count or sample format supported by this engine.
    Unsupported,
    /// The operating system denied access to audio.
    PermissionDenied,
    Other,
}

impl BackendError {
    /// Whether waiting and trying again can help.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::NoDevice | Self::Busy)
    }
}

/// A running output stream. Dropping it stops the stream.
pub trait RunningStream {}

/// An audio API. It is created and used on the engine thread only, so it need not be `Send` (some platform streams are not).
pub trait AudioBackend {
    /// Opens the system default output, asks `make_render` for the callback once the format is known, starts the stream and
    /// returns it with its format. Stream errors are reported through `events`.
    fn open_default_output(
        &mut self,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError>;

    /// A stable identity of the current default output device, or `None` when there is none.
    fn default_output_id(&mut self) -> Option<String>;

    /// Opens a selected local monitor. Backends without device selection report `Unsupported`.
    fn open_selected_output(
        &mut self,
        _id: &str,
        _make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        _events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
        Err(BackendError::Unsupported)
    }

    /// Opens microphone capture and a selected virtual output on this host thread.
    /// Both streams are owned by the returned handle and stop when it is dropped.
    fn open_route(
        &mut self,
        _devices: &RouteDevices,
        _make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        _events: EventSink,
    ) -> OpenRouteResult {
        Err(BackendError::Unsupported)
    }

    fn route_devices_available(&mut self, _devices: &RouteDevices) -> bool {
        true
    }
}
