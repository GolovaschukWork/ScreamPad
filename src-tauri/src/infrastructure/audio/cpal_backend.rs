//! The `cpal` implementation of the audio backend port (ADR 0006).
//!
//! The stream is created, played and dropped on the engine thread only, because some platform streams are not `Send`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, ErrorKind, Host, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};

use crate::audio::backend::{
    AudioBackend, BackendError, EventSink, OpenRouteResult, OutputFormat, RenderFn, RouteDevices,
    RunningStream, StreamEvent,
};

use super::cpal_route;

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
        open_output(device, make_render, events)
    }

    fn default_output_id(&mut self) -> Option<String> {
        let device = self.host.default_output_device()?;
        device.id().ok().map(|id| id.to_string())
    }

    fn open_selected_output(
        &mut self,
        id: &str,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
        let device = self
            .host
            .output_devices()
            .map_err(|_| BackendError::NoDevice)?
            .find(|device| device.id().is_ok_and(|found| found.to_string() == id))
            .ok_or(BackendError::NoDevice)?;
        open_output(device, make_render, events)
    }

    fn open_route(
        &mut self,
        devices: &RouteDevices,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> OpenRouteResult {
        cpal_route::open(&self.host, devices, make_render, events)
    }

    fn route_devices_available(&mut self, devices: &RouteDevices) -> bool {
        cpal_route::devices_available(&self.host, devices)
    }
}

fn open_output(
    device: Device,
    make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
    events: EventSink,
) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
    let supported = output_config(&device)?;
    let config: StreamConfig = supported.config();
    if !(1..=32).contains(&config.channels) {
        return Err(BackendError::Unsupported);
    }
    let format = OutputFormat {
        sample_rate: config.sample_rate,
        channels: config.channels,
    };

    let render = make_render(format);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_output::<f32>(&device, config, render, events),
        SampleFormat::I16 => build_output::<i16>(&device, config, render, events),
        SampleFormat::U16 => build_output::<u16>(&device, config, render, events),
        _ => Err(BackendError::Unsupported),
    }?;
    // Streams do not start by themselves in cpal 0.18.
    stream.play().map_err(|error| map_error(error.kind()))?;
    Ok((Box::new(CpalStream { _stream: stream }), format))
}

fn build_output<T: cpal_route::Pcm>(
    device: &Device,
    config: StreamConfig,
    mut render: RenderFn,
    events: EventSink,
) -> Result<Stream, BackendError> {
    let channels = usize::from(config.channels);
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data, _| {
                let mut scratch = [0.0_f32; 2048];
                let chunk_size = scratch.len() / channels * channels;
                for chunk in data.chunks_mut(chunk_size) {
                    let len = chunk.len();
                    render(&mut scratch[..len]);
                    for (target, source) in chunk.iter_mut().zip(&scratch[..len]) {
                        *target = T::from_float(*source);
                    }
                }
            },
            move |error| events(map_event(error.kind())),
            None,
        )
        .map_err(|error| map_error(error.kind()))
}

/// Prefer the existing float configuration; convert from the internal float mixer when a device only accepts integer PCM.
fn output_config(device: &Device) -> Result<SupportedStreamConfig, BackendError> {
    let default = device
        .default_output_config()
        .map_err(|error| map_error(error.kind()))?;
    if matches!(
        default.sample_format(),
        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
    ) {
        return Ok(default);
    }
    let ranges = device
        .supported_output_configs()
        .map_err(|error| map_error(error.kind()))?;
    for range in ranges {
        let rate = default.sample_rate();
        let fits = matches!(
            range.sample_format(),
            SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
        ) && range.channels() == default.channels()
            && (range.min_sample_rate()..=range.max_sample_rate()).contains(&rate);
        if fits {
            return Ok(range.with_sample_rate(rate));
        }
    }
    Err(BackendError::Unsupported)
}

pub(super) fn map_event(kind: ErrorKind) -> StreamEvent {
    match kind {
        ErrorKind::DeviceNotAvailable => StreamEvent::DeviceLost,
        ErrorKind::StreamInvalidated => StreamEvent::Invalidated,
        ErrorKind::DeviceChanged => StreamEvent::DefaultChanged,
        ErrorKind::Xrun => StreamEvent::Xrun,
        _ => StreamEvent::Failed,
    }
}

pub(super) fn map_error(kind: ErrorKind) -> BackendError {
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
