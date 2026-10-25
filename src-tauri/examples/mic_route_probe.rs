//! Standalone, opt-in CPAL routing experiment. This is not part of the Tauri application.
//!
//! cargo run --example mic_route_probe -- list
//! cargo run --example mic_route_probe -- start --virtual "BlackHole 2ch" --monitor "MacBook Pro Speakers" --mode both --seconds 600

use std::env;
use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, DeviceType, Host, SampleFormat, SizedSample, Stream, StreamConfig,
    SupportedStreamConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};

const QUEUE_MS: usize = 200;
const TARGET_MS: usize = 40;
const TONE_HZ: f32 = 523.25;
const TONE_GAIN: f32 = 0.12;

type ProbeResult<T> = Result<T, String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Mic,
    Clip,
    Both,
}

impl Mode {
    fn mic(self) -> bool {
        self != Self::Clip
    }
    fn clip(self) -> bool {
        self != Self::Mic
    }
}

#[derive(Default)]
struct Metrics {
    captured: AtomicU64,
    consumed: AtomicU64,
    virtual_frames: AtomicU64,
    overflow: AtomicU64,
    underflow: AtomicU64,
    discarded: AtomicU64,
    max_occupancy: AtomicUsize,
    input_callbacks: AtomicU64,
    virtual_callbacks: AtomicU64,
    monitor_callbacks: AtomicU64,
    input_max_frames: AtomicUsize,
    virtual_max_frames: AtomicUsize,
    monitor_max_frames: AtomicUsize,
    failed: AtomicBool,
}

impl Metrics {
    fn occupancy_max(&self, value: usize) {
        self.max_occupancy.fetch_max(value, Ordering::Relaxed);
    }
    fn report(
        &self,
        elapsed: Duration,
        occupancy: usize,
        capacity: usize,
        clock_drift_frames: f64,
    ) {
        println!(
            "t={:.0}s occupancy={occupancy}/{capacity} max={} captured={} consumed={} overflow={} underflow={} discarded={} callbacks(in/virtual/monitor)={}/{}/{} max_buffer_frames(in/virtual/monitor)={}/{}/{} clock_drift_frames={clock_drift_frames:.1}",
            elapsed.as_secs_f64(),
            self.max_occupancy.load(Ordering::Relaxed),
            self.captured.load(Ordering::Relaxed),
            self.consumed.load(Ordering::Relaxed),
            self.overflow.load(Ordering::Relaxed),
            self.underflow.load(Ordering::Relaxed),
            self.discarded.load(Ordering::Relaxed),
            self.input_callbacks.load(Ordering::Relaxed),
            self.virtual_callbacks.load(Ordering::Relaxed),
            self.monitor_callbacks.load(Ordering::Relaxed),
            self.input_max_frames.load(Ordering::Relaxed),
            self.virtual_max_frames.load(Ordering::Relaxed),
            self.monitor_max_frames.load(Ordering::Relaxed),
        );
    }
}

trait Pcm: SizedSample + Copy + Send + 'static {
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
        let value = f32::from_float(value);
        if value <= -1.0 {
            i16::MIN
        } else {
            (value * 32767.0).round() as i16
        }
    }
}
impl Pcm for u16 {
    fn to_float(self) -> f32 {
        (f32::from(self) - 32768.0) / 32768.0
    }
    fn from_float(value: f32) -> Self {
        ((f32::from_float(value) + 1.0) * 32767.5).round() as u16
    }
}

fn mono<T: Pcm>(frame: &[T]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    frame.iter().map(|sample| sample.to_float()).sum::<f32>() / frame.len() as f32
}

fn push_capture(tx: &mut Producer<f32>, sample: f32, metrics: &Metrics, capacity: usize) {
    if tx.push(sample).is_err() {
        metrics.overflow.fetch_add(1, Ordering::Relaxed);
    } else {
        metrics.captured.fetch_add(1, Ordering::Relaxed);
    }
    metrics.occupancy_max(capacity - tx.slots());
}

fn choose_device(host: &Host, name: &str, input: bool) -> ProbeResult<Device> {
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|error| format!("device enumeration: {error}"))?;
    let matches: Vec<Device> = devices
        .filter(|device| device.to_string() == name)
        .collect();
    match matches.as_slice() {
        [device] => Ok(device.clone()),
        [] => Err(format!("device '{name}' was not found; run 'list'")),
        _ => Err(format!(
            "device name '{name}' is ambiguous; use a unique device name"
        )),
    }
}

fn supported_config(device: &Device, input: bool) -> ProbeResult<SupportedStreamConfig> {
    let config = if input {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .map_err(|error| format!("configuration for {device}: {error}"))?;
    if !matches!(
        config.sample_format(),
        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
    ) {
        return Err(format!(
            "unsupported sample format {:?} on {device}",
            config.sample_format()
        ));
    }
    if !(1..=32).contains(&config.channels()) || !(8_000..=384_000).contains(&config.sample_rate())
    {
        return Err(format!("invalid device configuration on {device}"));
    }
    Ok(config)
}

fn physical_monitor(virtual_device: &Device, monitor: Option<Device>) -> Option<Device> {
    let device = monitor?;
    let (monitor_id, virtual_id) = match (device.id(), virtual_device.id()) {
        (Ok(monitor_id), Ok(virtual_id)) => (monitor_id, virtual_id),
        _ => {
            println!("Local monitoring disabled: device identity could not be verified.");
            return None;
        }
    };
    let kind = match device.description() {
        Ok(description) => description.device_type(),
        Err(error) => {
            println!("Local monitoring disabled: device type could not be verified: {error}");
            return None;
        }
    };
    if monitor_id == virtual_id {
        println!("Local monitoring disabled: selected output is BlackHole.");
        return None;
    }
    if !is_physical_output(kind) {
        println!(
            "Local monitoring disabled: selected output is not identified as physical ({kind:?})."
        );
        return None;
    }
    Some(device)
}

fn is_physical_output(kind: DeviceType) -> bool {
    matches!(
        kind,
        DeviceType::Speaker | DeviceType::Headphones | DeviceType::Headset | DeviceType::Earpiece
    )
}

fn build_input<T: Pcm>(
    device: &Device,
    config: StreamConfig,
    mut tx: Producer<f32>,
    metrics: Arc<Metrics>,
) -> ProbeResult<Stream> {
    let channels = usize::from(config.channels);
    let capacity = config.sample_rate as usize * QUEUE_MS / 1000;
    let error_metrics = Arc::clone(&metrics);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data, _| {
                if metrics.failed.load(Ordering::Relaxed) {
                    return;
                }
                metrics.input_callbacks.fetch_add(1, Ordering::Relaxed);
                metrics
                    .input_max_frames
                    .fetch_max(data.len() / channels, Ordering::Relaxed);
                for frame in data.chunks_exact(channels) {
                    push_capture(&mut tx, mono(frame), &metrics, capacity);
                }
            },
            move |_| {
                error_metrics.failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|error| format!("open microphone {device}: {error}"))
}

struct MicReader {
    rx: Consumer<f32>,
    phase: f64,
    prev: f32,
    next: f32,
    nominal_step: f64,
    target: usize,
    capacity: usize,
    metrics: Arc<Metrics>,
}

impl MicReader {
    fn new(
        mut rx: Consumer<f32>,
        input_rate: u32,
        output_rate: u32,
        metrics: Arc<Metrics>,
    ) -> Self {
        let prev = rx
            .pop()
            .inspect(|_| {
                metrics.consumed.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap_or(0.0);
        let next = rx
            .pop()
            .inspect(|_| {
                metrics.consumed.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap_or(prev);
        Self {
            rx,
            phase: 0.0,
            prev,
            next,
            nominal_step: f64::from(input_rate) / f64::from(output_rate),
            target: input_rate as usize * TARGET_MS / 1000,
            capacity: input_rate as usize * QUEUE_MS / 1000,
            metrics,
        }
    }

    fn frame(&mut self) -> f32 {
        let occupancy = self.rx.slots();
        // The virtual output owns the playback clock. Correct up to +/-0.5% of the nominal
        // conversion ratio; hard reset an excessively old backlog rather than grow latency.
        if occupancy > self.capacity * 3 / 4 {
            while self.rx.slots() > self.target {
                if self.rx.pop().is_err() {
                    break;
                }
                self.metrics.discarded.fetch_add(1, Ordering::Relaxed);
            }
        }
        let error = occupancy as f64 - self.target as f64;
        let correction = (error / self.target.max(1) as f64 * 0.002).clamp(-0.005, 0.005);
        let sample = self.prev + (self.next - self.prev) * self.phase as f32;
        self.phase += self.nominal_step * (1.0 + correction);
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.prev = self.next;
            self.next = match self.rx.pop() {
                Ok(value) => {
                    self.metrics.consumed.fetch_add(1, Ordering::Relaxed);
                    value
                }
                Err(_) => {
                    self.metrics.underflow.fetch_add(1, Ordering::Relaxed);
                    0.0
                }
            };
        }
        sample
    }
}

fn make_clip(rate: u32) -> Arc<[f32]> {
    // A prebuilt ten-second clip: five seconds of tone followed by five seconds of silence.
    let frames = rate as usize * 10;
    (0..frames)
        .map(|index| {
            if index >= frames / 2 {
                0.0
            } else {
                (index as f32 * TAU * TONE_HZ / rate as f32).sin() * TONE_GAIN
            }
        })
        .collect::<Vec<_>>()
        .into()
}

fn write_output<T: Pcm>(
    data: &mut [T],
    channels: usize,
    clip: Option<&[f32]>,
    mut mic: Option<&mut MicReader>,
    cursor: &mut usize,
) {
    for frame in data.chunks_exact_mut(channels) {
        let voice = mic.as_deref_mut().map_or(0.0, MicReader::frame);
        let tone = clip.map_or(0.0, |samples| samples[*cursor % samples.len()]);
        let sample = T::from_float((voice + tone).clamp(-1.0, 1.0));
        for (index, channel) in frame.iter_mut().enumerate() {
            *channel = if index < 2 {
                sample
            } else {
                T::from_float(0.0)
            };
        }
        *cursor += 1;
    }
}

fn render_or_silence<T: Pcm>(
    data: &mut [T],
    channels: usize,
    clip: Option<&[f32]>,
    mic: Option<&mut MicReader>,
    cursor: &mut usize,
    failed: bool,
) {
    if failed {
        for sample in data {
            *sample = T::from_float(0.0);
        }
    } else {
        write_output(data, channels, clip, mic, cursor);
    }
}

fn build_output<T: Pcm>(
    device: &Device,
    config: StreamConfig,
    mut mic: Option<MicReader>,
    clip: bool,
    metrics: Arc<Metrics>,
    monitor: bool,
) -> ProbeResult<Stream> {
    let channels = usize::from(config.channels);
    let samples = clip.then(|| make_clip(config.sample_rate));
    let error_metrics = Arc::clone(&metrics);
    let mut cursor = 0;
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data, _| {
                let failed = metrics.failed.load(Ordering::Relaxed);
                if monitor {
                    metrics.monitor_callbacks.fetch_add(1, Ordering::Relaxed);
                    metrics
                        .monitor_max_frames
                        .fetch_max(data.len() / channels, Ordering::Relaxed);
                } else {
                    metrics.virtual_callbacks.fetch_add(1, Ordering::Relaxed);
                    metrics
                        .virtual_max_frames
                        .fetch_max(data.len() / channels, Ordering::Relaxed);
                    metrics
                        .virtual_frames
                        .fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
                }
                render_or_silence(
                    data,
                    channels,
                    samples.as_deref(),
                    mic.as_mut(),
                    &mut cursor,
                    failed,
                );
            },
            move |_| {
                error_metrics.failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|error| format!("open output {device}: {error}"))
}

fn open_input(
    device: &Device,
    supported: SupportedStreamConfig,
    tx: Producer<f32>,
    metrics: Arc<Metrics>,
) -> ProbeResult<Stream> {
    let format = supported.sample_format();
    let config = supported.config();
    match format {
        SampleFormat::F32 => build_input::<f32>(device, config, tx, metrics),
        SampleFormat::I16 => build_input::<i16>(device, config, tx, metrics),
        SampleFormat::U16 => build_input::<u16>(device, config, tx, metrics),
        _ => Err(format!("unsupported input format: {format:?}")),
    }
}

fn open_output(
    device: &Device,
    supported: SupportedStreamConfig,
    mic: Option<MicReader>,
    clip: bool,
    metrics: Arc<Metrics>,
    monitor: bool,
) -> ProbeResult<Stream> {
    let format = supported.sample_format();
    let config = supported.config();
    match format {
        SampleFormat::F32 => build_output::<f32>(device, config, mic, clip, metrics, monitor),
        SampleFormat::I16 => build_output::<i16>(device, config, mic, clip, metrics, monitor),
        SampleFormat::U16 => build_output::<u16>(device, config, mic, clip, metrics, monitor),
        _ => Err(format!("unsupported output format: {format:?}")),
    }
}

fn device_info(label: &str, device: &Device, config: &SupportedStreamConfig) {
    println!(
        "{label}: name={device} id={} format={:?} rate={} channels={} supported_buffer={:?}",
        device
            .id()
            .map_or_else(|_| "unknown".to_owned(), |id| id.to_string()),
        config.sample_format(),
        config.sample_rate(),
        config.channels(),
        config.buffer_size()
    );
}

fn stop_streams<I, V, M>(input: Option<I>, monitor: Option<M>, virtual_stream: V) {
    drop(input);
    drop(monitor);
    drop(virtual_stream);
}

fn list(host: &Host) -> ProbeResult<()> {
    println!("Input devices:");
    for device in host.input_devices().map_err(|error| error.to_string())? {
        println!(
            "  {} id={} type={:?}",
            device,
            device
                .id()
                .map_or_else(|_| "unknown".to_owned(), |id| id.to_string()),
            device
                .description()
                .map(|description| description.device_type())
                .ok()
        );
    }
    println!("Output devices:");
    for device in host.output_devices().map_err(|error| error.to_string())? {
        println!(
            "  {} id={} type={:?}",
            device,
            device
                .id()
                .map_or_else(|_| "unknown".to_owned(), |id| id.to_string()),
            device
                .description()
                .map(|description| description.device_type())
                .ok()
        );
    }
    Ok(())
}

fn run(
    host: &Host,
    virtual_name: &str,
    monitor_name: Option<&str>,
    mode: Mode,
    seconds: u64,
) -> ProbeResult<()> {
    let virtual_device = choose_device(host, virtual_name, false)?;
    if !virtual_device
        .to_string()
        .to_lowercase()
        .contains("blackhole")
    {
        return Err("virtual output must be a BlackHole device".into());
    }
    let requested_monitor = monitor_name.and_then(|name| match choose_device(host, name, false) {
        Ok(device) => Some(device),
        Err(error) => {
            println!("Local monitoring disabled: {error}");
            None
        }
    });
    let monitor = physical_monitor(&virtual_device, requested_monitor);
    if monitor.is_none() && monitor_name.is_none() {
        println!("Local monitoring disabled: no distinct physical output selected.");
    }
    let input = if mode.mic() {
        Some(
            host.default_input_device()
                .ok_or("no default microphone available")?,
        )
    } else {
        None
    };
    let virtual_config = supported_config(&virtual_device, false)?;
    device_info("virtual output", &virtual_device, &virtual_config);
    let monitor_config =
        monitor
            .as_ref()
            .and_then(|device| match supported_config(device, false) {
                Ok(config) => Some(config),
                Err(error) => {
                    println!("Local monitoring disabled: {error}");
                    None
                }
            });
    if let (Some(device), Some(config)) = (&monitor, &monitor_config) {
        device_info("physical monitor", device, config);
    }
    let input_config = input
        .as_ref()
        .map(|device| supported_config(device, true))
        .transpose()?;
    if let (Some(device), Some(config)) = (&input, &input_config) {
        device_info("microphone", device, config);
    }

    let metrics = Arc::new(Metrics::default());
    let input_rate = input_config.as_ref().map_or(
        virtual_config.sample_rate(),
        SupportedStreamConfig::sample_rate,
    );
    let capacity = input_rate as usize * QUEUE_MS / 1000;
    let (tx, rx) = RingBuffer::new(capacity);
    let input_stream = match (input.as_ref(), input_config) {
        (Some(device), Some(config)) => Some(open_input(device, config, tx, Arc::clone(&metrics))?),
        _ => {
            drop(tx);
            None
        }
    };
    let mic = if mode.mic() {
        Some(MicReader::new(
            rx,
            input_rate,
            virtual_config.sample_rate(),
            Arc::clone(&metrics),
        ))
    } else {
        None
    };
    let virtual_stream = open_output(
        &virtual_device,
        virtual_config,
        mic,
        mode.clip(),
        Arc::clone(&metrics),
        false,
    )?;
    let mut monitor_stream = match (monitor.as_ref(), monitor_config) {
        (Some(device), Some(config)) => match open_output(
            device,
            config,
            None,
            mode.clip(),
            Arc::clone(&metrics),
            true,
        ) {
            Ok(stream) => Some(stream),
            Err(error) => {
                println!("Local monitoring disabled: {error}");
                None
            }
        },
        _ => None,
    };
    // Explicit `start` command is the only path that plays streams or captures the microphone.
    println!(
        "actual callback buffer frames: virtual={:?} monitor={:?} input={:?}",
        virtual_stream.buffer_size().ok(),
        monitor_stream
            .as_ref()
            .and_then(|stream| stream.buffer_size().ok()),
        input_stream
            .as_ref()
            .and_then(|stream| stream.buffer_size().ok())
    );
    if let Some(stream) = &input_stream {
        stream
            .play()
            .map_err(|error| format!("start microphone: {error}"))?;
    }
    if mode.mic() {
        let target = input_rate as u64 * TARGET_MS as u64 / 1000;
        let deadline = Instant::now() + Duration::from_secs(3);
        while metrics.captured.load(Ordering::Relaxed) < target
            && Instant::now() < deadline
            && !metrics.failed.load(Ordering::Acquire)
        {
            thread::sleep(Duration::from_millis(5));
        }
        if metrics.captured.load(Ordering::Relaxed) < target {
            drop(input_stream);
            drop(monitor_stream);
            drop(virtual_stream);
            return Err("microphone did not deliver enough frames to start routing".into());
        }
    }
    virtual_stream
        .play()
        .map_err(|error| format!("start BlackHole: {error}"))?;
    if let Some(stream) = &monitor_stream
        && let Err(error) = stream.play()
    {
        println!("Local monitoring disabled: start failed: {error}");
        monitor_stream = None;
    }
    let start = Instant::now();
    let input_baseline =
        metrics.captured.load(Ordering::Relaxed) + metrics.overflow.load(Ordering::Relaxed);
    let virtual_baseline = metrics.virtual_frames.load(Ordering::Relaxed);
    while start.elapsed() < Duration::from_secs(seconds) {
        thread::sleep(Duration::from_secs(1));
        let captured = metrics.captured.load(Ordering::Relaxed);
        let consumed = metrics.consumed.load(Ordering::Relaxed);
        let occupancy =
            captured.saturating_sub(consumed + metrics.discarded.load(Ordering::Relaxed)) as usize;
        let input_frames =
            (captured + metrics.overflow.load(Ordering::Relaxed)).saturating_sub(input_baseline);
        let output_frames = metrics
            .virtual_frames
            .load(Ordering::Relaxed)
            .saturating_sub(virtual_baseline);
        let drift = input_frames as f64
            - output_frames as f64 * f64::from(input_rate)
                / f64::from(virtual_config.sample_rate());
        metrics.report(start.elapsed(), occupancy.min(capacity), capacity, drift);
        if metrics.failed.load(Ordering::Acquire) {
            stop_streams(input_stream, monitor_stream, virtual_stream);
            return Err("audio device failed; all routing streams stopped".into());
        }
    }
    stop_streams(input_stream, monitor_stream, virtual_stream);
    println!("Capture stopped; streams dropped and queue released.");
    Ok(())
}

fn parse_and_run() -> ProbeResult<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let host = cpal::default_host();
    if args.first().is_some_and(|arg| arg == "list") {
        return list(&host);
    }
    if args.first().is_none_or(|arg| arg != "start") {
        return Err("usage: mic_route_probe list | start --virtual NAME [--monitor NAME] [--mode mic|clip|both] [--seconds N]".into());
    }
    let mut virtual_name = None;
    let mut monitor = None;
    let mut mode = Mode::Both;
    let mut seconds = 600;
    let mut cursor = 1;
    while cursor < args.len() {
        let value = args
            .get(cursor + 1)
            .ok_or_else(|| format!("missing value for {}", args[cursor]))?;
        match args[cursor].as_str() {
            "--virtual" => virtual_name = Some(value.as_str()),
            "--monitor" => monitor = Some(value.as_str()),
            "--mode" => {
                mode = match value.as_str() {
                    "mic" => Mode::Mic,
                    "clip" => Mode::Clip,
                    "both" => Mode::Both,
                    _ => return Err("mode must be mic, clip or both".into()),
                }
            }
            "--seconds" => {
                seconds = value
                    .parse::<u64>()
                    .map_err(|_| "seconds must be a positive integer")?
            }
            other => return Err(format!("unknown option: {other}")),
        }
        cursor += 2;
    }
    if seconds == 0 {
        return Err("seconds must be positive".into());
    }
    run(
        &host,
        virtual_name.ok_or("--virtual is required")?,
        monitor,
        mode,
        seconds,
    )
}

fn main() {
    if let Err(error) = parse_and_run() {
        eprintln!("mic route probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_is_bounded_and_new_frames_are_dropped_when_full() {
        let (mut tx, mut rx) = RingBuffer::new(2);
        let metrics = Metrics::default();
        push_capture(&mut tx, 1.0, &metrics, 2);
        push_capture(&mut tx, 2.0, &metrics, 2);
        push_capture(&mut tx, 3.0, &metrics, 2);
        assert_eq!(rx.pop(), Ok(1.0));
        assert_eq!(rx.pop(), Ok(2.0));
        assert!(rx.pop().is_err());
        assert_eq!(metrics.captured.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.overflow.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.max_occupancy.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn clock_mismatch_does_not_make_queue_or_latency_unbounded() {
        let (mut tx, rx) = RingBuffer::new(200);
        let metrics = Arc::new(Metrics::default());
        let mut reader = MicReader::new(rx, 1_000, 1_000, Arc::clone(&metrics));
        // Ten simulated minutes at 1 kHz; capture supplies 1% more frames.
        for frame in 0..600_000 {
            push_capture(&mut tx, 0.25, &metrics, 200);
            if frame % 100 == 0 {
                push_capture(&mut tx, 0.25, &metrics, 200);
            }
            let _ = reader.frame();
        }
        assert!(metrics.max_occupancy.load(Ordering::Relaxed) <= 200);
        assert!(metrics.discarded.load(Ordering::Relaxed) > 0);
        assert!(reader.rx.slots() <= 200);
    }

    #[test]
    fn stereo_and_surround_capture_are_mono_averages() {
        assert!((mono(&[0.2_f32, 0.6]) - 0.4).abs() < 0.0001);
        assert!((mono(&[0.2_f32, 0.4, 0.6, 0.8]) - 0.5).abs() < 0.0001);
        assert_eq!(mono::<f32>(&[]), 0.0);
    }

    #[test]
    fn integer_formats_map_zero_and_endpoints_without_wraparound() {
        assert_eq!(i16::from_float(0.0), 0);
        assert_eq!(u16::from_float(0.0), 32768);
        assert_eq!(i16::from_float(2.0), i16::MAX);
        assert_eq!(u16::from_float(-2.0), 0);
        assert!((i16::MIN.to_float() + 1.0).abs() < 0.0001);
        assert!(f32::from_float(f32::NAN).is_finite());
    }

    #[test]
    fn monitor_never_receives_microphone_samples() {
        let mut output = [0.0_f32; 4];
        let mut cursor = 0;
        write_output(&mut output, 2, None, None, &mut cursor);
        assert_eq!(output, [0.0; 4]);
        assert_eq!(cursor, 2);
    }

    #[test]
    fn extra_output_channels_remain_silent() {
        let mut output = [0.0_f32; 4];
        let mut cursor = 0;
        write_output(&mut output, 4, Some(&[0.5]), None, &mut cursor);
        assert_eq!(output, [0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn only_known_physical_types_can_monitor() {
        assert!(is_physical_output(DeviceType::Speaker));
        assert!(is_physical_output(DeviceType::Headphones));
        assert!(!is_physical_output(DeviceType::Virtual));
        assert!(!is_physical_output(DeviceType::Unknown));
    }

    #[test]
    fn device_failure_flag_stops_routing_and_is_visible_to_control_loop() {
        let metrics = Metrics::default();
        assert!(!metrics.failed.load(Ordering::Acquire));
        metrics.failed.store(true, Ordering::Release);
        assert!(metrics.failed.load(Ordering::Acquire));
        let mut output = [0.5_f32; 2];
        let mut cursor = 0;
        render_or_silence(
            &mut output,
            2,
            Some(&[0.75]),
            None,
            &mut cursor,
            metrics.failed.load(Ordering::Acquire),
        );
        assert_eq!(output, [0.0, 0.0]);
        assert_eq!(cursor, 0, "failed routing cannot advance clip playback");
    }

    #[test]
    fn stopping_all_streams_releases_each_resource() {
        struct DropProbe(Arc<AtomicUsize>);
        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        stop_streams(
            Some(DropProbe(Arc::clone(&drops))),
            Some(DropProbe(Arc::clone(&drops))),
            DropProbe(Arc::clone(&drops)),
        );
        assert_eq!(drops.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn dropping_capture_endpoints_releases_buffered_samples() {
        let (mut tx, rx) = RingBuffer::new(2);
        tx.push(0.5).expect("space");
        drop(tx);
        drop(rx);
        let (mut fresh, mut empty) = RingBuffer::new(2);
        assert!(empty.pop().is_err());
        fresh.push(0.25).expect("space");
        assert_eq!(empty.pop(), Ok(0.25));
    }
}
