//! A fake audio API for tests: it lets a test decide what opening a stream returns, pump the audio callback by hand, and
//! raise stream events as an operating system would.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use super::backend::{
    AudioBackend, BackendError, EventSink, OpenRouteResult, OutputFormat, RenderFn, RouteDevices,
    RouteMetrics, RunningStream, StreamEvent,
};

pub const STEREO_48K: OutputFormat = OutputFormat {
    sample_rate: 48_000,
    channels: 2,
};

pub struct FakeShared {
    /// Every call to `open_default_output`, successful or not.
    pub open_calls: AtomicUsize,
    /// Streams currently open (dropped streams are subtracted).
    pub streams_alive: AtomicUsize,
    /// Results for the next opens, consumed in order; when empty, opening succeeds with `default_format`.
    pub plan: Mutex<VecDeque<Result<OutputFormat, BackendError>>>,
    pub default_format: Mutex<OutputFormat>,
    pub default_id: Mutex<Option<String>>,
    render: Mutex<Option<(RenderFn, OutputFormat)>>,
    sink: Mutex<Option<EventSink>>,
    route_render: Mutex<Option<(RenderFn, OutputFormat)>>,
    route_sink: Mutex<Option<EventSink>>,
    route_error: Mutex<Option<BackendError>>,
    route_missing_polls: AtomicUsize,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl FakeShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            open_calls: AtomicUsize::new(0),
            streams_alive: AtomicUsize::new(0),
            plan: Mutex::new(VecDeque::new()),
            default_format: Mutex::new(STEREO_48K),
            default_id: Mutex::new(Some("speakers".to_owned())),
            render: Mutex::new(None),
            sink: Mutex::new(None),
            route_render: Mutex::new(None),
            route_sink: Mutex::new(None),
            route_error: Mutex::new(None),
            route_missing_polls: AtomicUsize::new(0),
        })
    }

    pub fn open_calls(&self) -> usize {
        self.open_calls.load(Ordering::SeqCst)
    }

    pub fn streams_alive(&self) -> usize {
        self.streams_alive.load(Ordering::SeqCst)
    }

    pub fn plan(&self, results: impl IntoIterator<Item = Result<OutputFormat, BackendError>>) {
        lock(&self.plan).extend(results);
    }

    pub fn set_default_id(&self, id: Option<&str>) {
        *lock(&self.default_id) = id.map(str::to_owned);
    }

    pub fn set_default_format(&self, format: OutputFormat) {
        *lock(&self.default_format) = format;
    }

    /// Runs the audio callback of the current stream once, like the operating system would. `None` when no stream is open.
    pub fn pump(&self, frames: usize) -> Option<Vec<f32>> {
        let mut guard = lock(&self.render);
        let (render, format) = guard.as_mut()?;
        let mut out = vec![9.0; frames * usize::from(format.channels)];
        render(&mut out);
        Some(out)
    }

    /// The sink of the current stream, as an old stream's error callback would still hold it.
    pub fn sink(&self) -> Option<EventSink> {
        lock(&self.sink).clone()
    }

    pub fn pump_route(&self, frames: usize) -> Option<Vec<f32>> {
        let mut guard = lock(&self.route_render);
        let (render, format) = guard.as_mut()?;
        let mut out = vec![9.0; frames * usize::from(format.channels)];
        render(&mut out);
        Some(out)
    }

    pub fn emit_route(&self, event: StreamEvent) {
        if let Some(sink) = lock(&self.route_sink).clone() {
            sink(event);
        }
    }

    pub fn fail_next_route_with(&self, error: BackendError) {
        *lock(&self.route_error) = Some(error);
    }

    pub fn miss_route_devices_for_polls(&self, polls: usize) {
        self.route_missing_polls.store(polls, Ordering::SeqCst);
    }

    pub fn remaining_route_device_misses(&self) -> usize {
        self.route_missing_polls.load(Ordering::SeqCst)
    }

    /// Raises a stream event as the audio API's error callback would.
    pub fn emit(&self, event: StreamEvent) {
        let sink = lock(&self.sink).clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }
}

pub struct FakeBackend(pub Arc<FakeShared>);

struct FakeStream(Arc<FakeShared>);
struct FakeRouteStream(Arc<FakeShared>);

impl RunningStream for FakeStream {}
impl RunningStream for FakeRouteStream {}

impl Drop for FakeRouteStream {
    fn drop(&mut self) {
        self.0.streams_alive.fetch_sub(1, Ordering::SeqCst);
        *lock(&self.0.route_render) = None;
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.0.streams_alive.fetch_sub(1, Ordering::SeqCst);
        // A stopped stream no longer calls its callback.
        *lock(&self.0.render) = None;
    }
}

impl AudioBackend for FakeBackend {
    fn open_default_output(
        &mut self,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
        let shared = &self.0;
        shared.open_calls.fetch_add(1, Ordering::SeqCst);
        let planned = lock(&shared.plan).pop_front();
        let format = match planned {
            Some(Err(error)) => return Err(error),
            Some(Ok(format)) => format,
            None => *lock(&shared.default_format),
        };
        let render = make_render(format);
        *lock(&shared.render) = Some((render, format));
        *lock(&shared.sink) = Some(events);
        shared.streams_alive.fetch_add(1, Ordering::SeqCst);
        Ok((Box::new(FakeStream(Arc::clone(shared))), format))
    }

    fn default_output_id(&mut self) -> Option<String> {
        lock(&self.0.default_id).clone()
    }

    fn open_selected_output(
        &mut self,
        id: &str,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
        if id == "virtual" {
            return Err(BackendError::Unsupported);
        }
        self.open_default_output(make_render, events)
    }

    fn open_route(
        &mut self,
        devices: &RouteDevices,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        events: EventSink,
    ) -> OpenRouteResult {
        if let Some(error) = lock(&self.0.route_error).take() {
            return Err(error);
        }
        if devices.microphone_id == devices.virtual_output_id {
            return Err(BackendError::Unsupported);
        }
        let format = STEREO_48K;
        *lock(&self.0.route_render) = Some((make_render(format), format));
        *lock(&self.0.route_sink) = Some(events);
        self.0.streams_alive.fetch_add(1, Ordering::SeqCst);
        Ok((
            Box::new(FakeRouteStream(Arc::clone(&self.0))),
            format,
            Arc::new(RouteMetrics::default()),
        ))
    }

    fn route_devices_available(&mut self, _devices: &RouteDevices) -> bool {
        self.0
            .route_missing_polls
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_err()
    }
}
