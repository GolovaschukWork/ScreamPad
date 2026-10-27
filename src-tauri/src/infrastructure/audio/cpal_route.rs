//! Selected CPAL capture/output pair. The existing audio worker owns both streams.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, DeviceType, Host, SampleFormat, Stream, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio::backend::{
    BackendError, EventSink, OpenRouteResult, OutputFormat, RenderFn, RouteDevices, RouteMetrics,
    RunningStream, StreamEvent,
};

const QUEUE_MS: usize = 200;
const TARGET_MS: usize = 40;
const SCRATCH_SAMPLES: usize = 2048;
const METER_SCALE: f64 = 100_000_000.0;

struct RouteStreams {
    output: Stream,
    input: Stream,
}

impl RunningStream for RouteStreams {}

// `Stream` is kept alive by its owner on the engine thread. Dropping the output before the input
// prevents a late output callback from reading an about-to-be-destroyed capture queue.
impl Drop for RouteStreams {
    fn drop(&mut self) {
        let _ = self.output.pause();
        let _ = self.input.pause();
    }
}

pub(super) trait Pcm: cpal::SizedSample + Copy + Send + 'static {
    fn to_float(self) -> f32;
    fn from_float(value: f32) -> Self;
}

impl Pcm for f32 {
    fn to_float(self) -> f32 {
        if self.is_finite() {
            self.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
    fn from_float(value: f32) -> Self {
        if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
}
impl Pcm for i16 {
    fn to_float(self) -> f32 {
        f32::from(self) / 32768.0
    }
    fn from_float(value: f32) -> Self {
        (f32::from_float(value) * 32767.0).round() as i16
    }
}
impl Pcm for u16 {
    fn to_float(self) -> f32 {
        (f32::from(self) - 0.0 - 32768.0) / 32768.0
    }
    fn from_float(value: f32) -> Self {
        ((f32::from_float(value) + 1.0) * 32767.5).round() as u16
    }
}

fn mono<T: Pcm>(frame: &[T]) -> f32 {
    frame.iter().map(|sample| sample.to_float()).sum::<f32>() / frame.len().max(1) as f32
}

fn enqueue_sample(tx: &mut Producer<f32>, sample: f32) -> bool {
    tx.push(sample).is_ok()
}

fn mix_frame<T: Pcm>(output: &mut [T], clip: &[f32], microphone: f32) -> u64 {
    let mut clipped = 0;
    for (target, sample) in output.iter_mut().zip(clip) {
        let combined = microphone + sample;
        if combined.abs() > 1.0 {
            clipped += 1;
        }
        *target = T::from_float(combined);
    }
    clipped
}

struct MicReader {
    rx: Consumer<f32>,
    phase: f64,
    previous: f32,
    next: f32,
    step: f64,
    target: usize,
    capacity: usize,
    metrics: Arc<RouteMetrics>,
}

impl MicReader {
    fn new(
        rx: Consumer<f32>,
        input_rate: u32,
        output_rate: u32,
        metrics: Arc<RouteMetrics>,
    ) -> Self {
        Self {
            rx,
            phase: 0.0,
            previous: 0.0,
            next: 0.0,
            step: f64::from(input_rate) / f64::from(output_rate),
            target: (input_rate as usize * TARGET_MS / 1000).max(1),
            capacity: (input_rate as usize * QUEUE_MS / 1000).max(1),
            metrics,
        }
    }

    fn frame(&mut self) -> f32 {
        let occupancy = self.rx.slots();
        self.metrics
            .occupancy
            .store(occupancy as u64, Ordering::Relaxed);
        self.metrics
            .max_occupancy
            .fetch_max(occupancy as u64, Ordering::Relaxed);
        if occupancy > self.capacity * 3 / 4 {
            while self.rx.slots() > self.target {
                if self.rx.pop().is_err() {
                    break;
                }
                self.metrics.discarded.fetch_add(1, Ordering::Relaxed);
            }
        }
        let error = occupancy as f64 - self.target as f64;
        let correction = (error / self.target as f64 * 0.002).clamp(-0.005, 0.005);
        let value = self.previous + (self.next - self.previous) * self.phase as f32;
        self.phase += self.step * (1.0 + correction);
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.previous = self.next;
            self.next = match self.rx.pop() {
                Ok(sample) => sample,
                Err(_) => {
                    self.metrics.underflow.fetch_add(1, Ordering::Relaxed);
                    0.0
                }
            };
        }
        value
    }
}

fn capture<T: Pcm>(
    device: &Device,
    supported: SupportedStreamConfig,
    mut tx: Producer<f32>,
    metrics: Arc<RouteMetrics>,
    events: EventSink,
) -> Result<Stream, BackendError> {
    let config: StreamConfig = supported.config();
    let channels = usize::from(config.channels);
    let errors = Arc::clone(&metrics);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data, _| {
                if metrics.failed.load(Ordering::Relaxed) {
                    return;
                }
                let mut accepted = 0;
                let mut overflow = 0;
                for frame in data.chunks_exact(channels) {
                    if !enqueue_sample(&mut tx, mono(frame)) {
                        overflow += 1;
                    } else {
                        accepted += 1;
                    }
                }
                metrics.captured.fetch_add(accepted, Ordering::Relaxed);
                metrics.overflow.fetch_add(overflow, Ordering::Relaxed);
            },
            move |error| {
                let event = super::cpal_backend::map_event(error.kind());
                if event == StreamEvent::Xrun {
                    errors.xruns.fetch_add(1, Ordering::Relaxed);
                } else {
                    errors.failed.store(true, Ordering::Release);
                }
                events(event);
            },
            None,
        )
        .map_err(|error| super::cpal_backend::map_error(error.kind()))
}

fn render_output<T: Pcm>(
    device: &Device,
    supported: SupportedStreamConfig,
    mut render: RenderFn,
    mut mic: MicReader,
    metrics: Arc<RouteMetrics>,
    events: EventSink,
) -> Result<Stream, BackendError> {
    let config: StreamConfig = supported.config();
    let channels = usize::from(config.channels);
    let errors = Arc::clone(&metrics);
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data, _| {
                if metrics.failed.load(Ordering::Acquire) {
                    data.fill(T::from_float(0.0));
                    return;
                }
                let mut scratch = [0.0_f32; SCRATCH_SAMPLES];
                let mut clipped = 0;
                let mut mic_energy = 0.0_f64;
                let mut clip_energy = 0.0_f64;
                let mut output_energy = 0.0_f64;
                let mut output_peak = 0.0_f64;
                for chunk in data.chunks_mut(SCRATCH_SAMPLES) {
                    let len = chunk.len();
                    render(&mut scratch[..len]);
                    for (frame, clip) in chunk
                        .chunks_exact_mut(channels)
                        .zip(scratch[..len].chunks_exact(channels))
                    {
                        let voice = mic.frame();
                        mic_energy += f64::from(voice * voice);
                        for sample in clip {
                            clip_energy += f64::from(sample * sample) / channels as f64;
                            let output = (voice + sample).clamp(-1.0, 1.0);
                            output_energy += f64::from(output * output) / channels as f64;
                            output_peak = output_peak.max(f64::from(output.abs()));
                        }
                        clipped += mix_frame(frame, clip, voice);
                    }
                }
                metrics.clipped.fetch_add(clipped, Ordering::Relaxed);
                metrics
                    .rendered
                    .fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
                metrics
                    .mic_energy
                    .fetch_add((mic_energy * METER_SCALE) as u64, Ordering::Relaxed);
                metrics
                    .clip_energy
                    .fetch_add((clip_energy * METER_SCALE) as u64, Ordering::Relaxed);
                metrics
                    .output_energy
                    .fetch_add((output_energy * METER_SCALE) as u64, Ordering::Relaxed);
                metrics
                    .output_peak
                    .fetch_max((output_peak * METER_SCALE) as u64, Ordering::Relaxed);
            },
            move |error| {
                let event = super::cpal_backend::map_event(error.kind());
                if event == StreamEvent::Xrun {
                    errors.xruns.fetch_add(1, Ordering::Relaxed);
                } else {
                    errors.failed.store(true, Ordering::Release);
                }
                events(event);
            },
            None,
        )
        .map_err(|error| super::cpal_backend::map_error(error.kind()))
}

fn find_device(host: &Host, id: &str, input: bool) -> Result<Device, BackendError> {
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|_| BackendError::NoDevice)?;
    devices
        .into_iter()
        .find(|device| device.id().is_ok_and(|found| found.to_string() == id))
        .ok_or(BackendError::NoDevice)
}

pub fn devices_available(host: &Host, devices: &RouteDevices) -> bool {
    find_device(host, &devices.microphone_id, true).is_ok()
        && find_device(host, &devices.virtual_output_id, false).is_ok()
        && find_device(host, &devices.virtual_output_id, true).is_ok()
}

fn compatible_config(device: &Device, input: bool) -> Result<SupportedStreamConfig, BackendError> {
    let default = if input {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .map_err(|error| super::cpal_backend::map_error(error.kind()))?;
    let compatible = |channels, format| {
        (1..=2).contains(&channels)
            && matches!(
                format,
                SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
            )
    };
    if compatible(default.channels(), default.sample_format()) {
        return Ok(default);
    }
    let selected = if input {
        choose_range(
            device
                .supported_input_configs()
                .map_err(|error| super::cpal_backend::map_error(error.kind()))?,
            default.sample_rate(),
        )
    } else {
        choose_range(
            device
                .supported_output_configs()
                .map_err(|error| super::cpal_backend::map_error(error.kind()))?,
            default.sample_rate(),
        )
    };
    selected.ok_or(BackendError::Unsupported)
}

fn choose_range(
    ranges: impl Iterator<Item = SupportedStreamConfigRange>,
    preferred: u32,
) -> Option<SupportedStreamConfig> {
    for range in ranges {
        if !(1..=2).contains(&range.channels())
            || !matches!(
                range.sample_format(),
                SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
            )
        {
            continue;
        }
        let supported = range.min_sample_rate()..=range.max_sample_rate();
        let rate = if supported.contains(&preferred) {
            preferred
        } else if supported.contains(&48_000) {
            48_000
        } else {
            range.min_sample_rate()
        };
        if (8_000..=192_000).contains(&rate) {
            return Some(range.with_sample_rate(rate));
        }
    }
    None
}

pub fn open(
    host: &Host,
    devices: &RouteDevices,
    make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
    events: EventSink,
) -> OpenRouteResult {
    if devices.microphone_id == devices.virtual_output_id {
        return Err(BackendError::Unsupported);
    }
    let input = find_device(host, &devices.microphone_id, true)?;
    if input
        .description()
        .is_ok_and(|description| description.device_type() == DeviceType::Virtual)
    {
        return Err(BackendError::Unsupported);
    }
    let output = find_device(host, &devices.virtual_output_id, false)?;
    // A chat application needs to see the matching virtual input.
    find_device(host, &devices.virtual_output_id, true)?;
    let input_config = compatible_config(&input, true)?;
    let output_config = compatible_config(&output, false)?;
    let format = OutputFormat {
        sample_rate: output_config.sample_rate(),
        channels: output_config.channels(),
    };
    let capacity = (input_config.sample_rate() as usize * QUEUE_MS / 1000).max(1);
    let (tx, rx) = RingBuffer::new(capacity);
    let metrics = Arc::new(RouteMetrics {
        input_rate: input_config.sample_rate(),
        output_rate: output_config.sample_rate(),
        queue_capacity: capacity as u32,
        ..RouteMetrics::default()
    });
    let input_stream = match input_config.sample_format() {
        SampleFormat::F32 => capture::<f32>(
            &input,
            input_config,
            tx,
            Arc::clone(&metrics),
            Arc::clone(&events),
        ),
        SampleFormat::I16 => capture::<i16>(
            &input,
            input_config,
            tx,
            Arc::clone(&metrics),
            Arc::clone(&events),
        ),
        SampleFormat::U16 => capture::<u16>(
            &input,
            input_config,
            tx,
            Arc::clone(&metrics),
            Arc::clone(&events),
        ),
        _ => return Err(BackendError::Unsupported),
    }?;
    let mic = MicReader::new(
        rx,
        input_config.sample_rate(),
        output_config.sample_rate(),
        Arc::clone(&metrics),
    );
    let render = make_render(format);
    let output_stream = match output_config.sample_format() {
        SampleFormat::F32 => render_output::<f32>(
            &output,
            output_config,
            render,
            mic,
            Arc::clone(&metrics),
            events,
        ),
        SampleFormat::I16 => render_output::<i16>(
            &output,
            output_config,
            render,
            mic,
            Arc::clone(&metrics),
            events,
        ),
        SampleFormat::U16 => render_output::<u16>(
            &output,
            output_config,
            render,
            mic,
            Arc::clone(&metrics),
            events,
        ),
        _ => return Err(BackendError::Unsupported),
    }?;
    metrics.input_buffer_frames.store(
        u64::from(input_stream.buffer_size().unwrap_or(0)),
        Ordering::Relaxed,
    );
    metrics.output_buffer_frames.store(
        u64::from(output_stream.buffer_size().unwrap_or(0)),
        Ordering::Relaxed,
    );
    input_stream
        .play()
        .map_err(|error| super::cpal_backend::map_error(error.kind()))?;
    output_stream
        .play()
        .map_err(|error| super::cpal_backend::map_error(error.kind()))?;
    if metrics.failed.load(Ordering::Acquire) {
        return Err(BackendError::Other);
    }
    Ok((
        Box::new(RouteStreams {
            output: output_stream,
            input: input_stream,
        }),
        format,
        metrics,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_and_mono_are_bounded() {
        assert_eq!(mono(&[i16::MAX, i16::MIN]), -1.0 / 65536.0);
        assert_eq!(mono(&[u16::MIN, u16::MAX]), -1.0 / 65536.0);
        assert!((f32::from_float(1.3) - 1.0).abs() < f32::EPSILON);
        assert_eq!(i16::from_float(-2.0), i16::MIN + 1);
        assert!((u16::from_float(0.0) as i32 - 32768).abs() <= 1);
    }

    #[test]
    fn microphone_and_clip_sum_and_clip_in_each_supported_sample_format() {
        let mut stereo = [0.0_f32; 2];
        assert_eq!(mix_frame(&mut stereo, &[0.2, -0.2], 0.3), 0);
        assert!((stereo[0] - 0.5).abs() < 1e-6);
        assert!((stereo[1] - 0.1).abs() < 1e-6);
        assert_eq!(mix_frame(&mut stereo, &[0.8, 0.8], 0.5), 2);
        assert_eq!(stereo, [1.0, 1.0]);
        let mut mono_i16 = [0_i16];
        mix_frame(&mut mono_i16, &[0.25], 0.25);
        assert!((i32::from(mono_i16[0]) - 16384).abs() <= 1);
        let mut mono_u16 = [0_u16];
        mix_frame(&mut mono_u16, &[0.25], 0.25);
        assert!((i32::from(mono_u16[0]) - 49151).abs() <= 1);
    }

    #[test]
    fn bounded_queue_discards_old_backlog() {
        let (mut tx, rx) = RingBuffer::new(9600);
        for _ in 0..9000 {
            tx.push(0.5).expect("space");
        }
        let metrics = Arc::new(RouteMetrics::default());
        let mut reader = MicReader::new(rx, 48_000, 48_000, Arc::clone(&metrics));
        assert!(reader.frame() >= 0.0);
        assert!(metrics.discarded.load(Ordering::Relaxed) > 0);
        assert!(reader.rx.slots() <= reader.target);
    }

    #[test]
    fn overflow_drops_newest_frame_and_keeps_existing_audio() {
        let (mut tx, mut rx) = RingBuffer::new(2);
        assert!(enqueue_sample(&mut tx, 0.1));
        assert!(enqueue_sample(&mut tx, 0.2));
        assert!(!enqueue_sample(&mut tx, 0.3));
        assert_eq!(rx.pop(), Ok(0.1));
        assert_eq!(rx.pop(), Ok(0.2));
        assert!(rx.pop().is_err());
    }
}
