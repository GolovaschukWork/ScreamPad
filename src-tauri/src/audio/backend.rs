//! The port between the audio engine and an audio API. The engine never names `cpal`; tests use a fake backend.

use std::sync::Arc;

/// Called from the audio thread with an interleaved buffer to fill. Must not allocate, block or log.
pub type RenderFn = Box<dyn FnMut(&mut [f32]) + Send>;

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
    /// The device cannot run a format the engine can use (the engine needs 32-bit float output).
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
}
