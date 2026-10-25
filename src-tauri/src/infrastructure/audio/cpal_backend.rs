//! The `cpal` implementation of the audio backend port (ADR 0006).
//!
//! The stream is created, played and dropped on the engine thread only, because some platform streams are not `Send`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, ErrorKind, Host, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};

use crate::audio::backend::{
    AudioBackend, BackendError, EventSink, OutputFormat, RenderFn, RunningStream, StreamEvent,
};

pub struct CpalBackend {
    host: Host,
}

impl CpalBackend {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }
}

impl Default for CpalBackend {
    fn default() -> Self {
        Self::new()
    }
}

struct CpalStream {
    // Held only so that dropping it stops the stream.
    _stream: Stream,
}

impl RunningStream for CpalStream {}

impl AudioBackend for CpalBackend {
    fn open_default_output(
        &mut self,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
        let device = self
            .host
            .default_output_device()
            .ok_or(BackendError::NoDevice)?;
        let supported = float_config(&device)?;
        let config: StreamConfig = supported.config();
        let format = OutputFormat {
            sample_rate: config.sample_rate,
            channels: config.channels,
        };

        let mut render = make_render(format);
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |data, _| render(data),
                move |error| events(map_event(error.kind())),
                None,
            )
            .map_err(|error| map_error(error.kind()))?;
        // Streams do not start by themselves in cpal 0.18.
        stream.play().map_err(|error| map_error(error.kind()))?;
        Ok((Box::new(CpalStream { _stream: stream }), format))
    }

    fn default_output_id(&mut self) -> Option<String> {
        let device = self.host.default_output_device()?;
        device.id().ok().map(|id| id.to_string())
    }
}

/// The engine mixes in 32-bit float, so the device must offer it: the default configuration when it is float, otherwise a
/// float configuration at the default rate and channel count. Other sample formats are not converted yet.
fn float_config(device: &Device) -> Result<SupportedStreamConfig, BackendError> {
    let default = device
        .default_output_config()
        .map_err(|error| map_error(error.kind()))?;
    if default.sample_format() == SampleFormat::F32 {
        return Ok(default);
    }
    let ranges = device
        .supported_output_configs()
        .map_err(|error| map_error(error.kind()))?;
    for range in ranges {
        let rate = default.sample_rate();
        let fits = range.sample_format() == SampleFormat::F32
            && range.channels() == default.channels()
            && (range.min_sample_rate()..=range.max_sample_rate()).contains(&rate);
        if fits {
            return Ok(range.with_sample_rate(rate));
        }
    }
    Err(BackendError::Unsupported)
}

fn map_event(kind: ErrorKind) -> StreamEvent {
    match kind {
        ErrorKind::DeviceNotAvailable => StreamEvent::DeviceLost,
        ErrorKind::StreamInvalidated => StreamEvent::Invalidated,
        ErrorKind::DeviceChanged => StreamEvent::DefaultChanged,
        ErrorKind::Xrun => StreamEvent::Xrun,
        _ => StreamEvent::Failed,
    }
}

fn map_error(kind: ErrorKind) -> BackendError {
    match kind {
        ErrorKind::DeviceNotAvailable | ErrorKind::HostUnavailable => BackendError::NoDevice,
        ErrorKind::DeviceBusy | ErrorKind::ResourceExhausted => BackendError::Busy,
        ErrorKind::PermissionDenied => BackendError::PermissionDenied,
        ErrorKind::UnsupportedConfig
        | ErrorKind::UnsupportedOperation
        | ErrorKind::InvalidInput => BackendError::Unsupported,
        _ => BackendError::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_errors_map_to_engine_events() {
        assert_eq!(
            map_event(ErrorKind::DeviceNotAvailable),
            StreamEvent::DeviceLost
        );
        assert_eq!(
            map_event(ErrorKind::StreamInvalidated),
            StreamEvent::Invalidated
        );
        assert_eq!(
            map_event(ErrorKind::DeviceChanged),
            StreamEvent::DefaultChanged
        );
        assert_eq!(map_event(ErrorKind::Xrun), StreamEvent::Xrun);
        assert_eq!(map_event(ErrorKind::BackendError), StreamEvent::Failed);
    }

    #[test]
    fn open_errors_map_to_retryable_or_final_failures() {
        assert!(map_error(ErrorKind::DeviceNotAvailable).is_retryable());
        assert!(map_error(ErrorKind::HostUnavailable).is_retryable());
        assert!(map_error(ErrorKind::DeviceBusy).is_retryable());
        assert!(map_error(ErrorKind::ResourceExhausted).is_retryable());
        assert_eq!(
            map_error(ErrorKind::PermissionDenied),
            BackendError::PermissionDenied
        );
        assert_eq!(
            map_error(ErrorKind::UnsupportedConfig),
            BackendError::Unsupported
        );
        assert!(!map_error(ErrorKind::PermissionDenied).is_retryable());
        assert!(!map_error(ErrorKind::Other).is_retryable());
    }
}
