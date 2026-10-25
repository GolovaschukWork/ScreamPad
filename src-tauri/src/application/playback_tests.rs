//! Playback tests: real library adapters on a temporary directory, a fake audio API, and a decoder wrapper that counts calls.

use std::fs;
use std::fs::File;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::cancel::CancelToken;
use super::import::ImportOutcome;
use super::playback::{PlaybackError, PlaybackLimits, PlaybackService};
use super::ports::{ClipDecoder, ProbeError, ProbeLimits};
use super::test_env::Env;
use crate::audio::backend::{BackendError, OutputFormat, StreamEvent};
use crate::audio::clip::Clip;
use crate::audio::engine::{AudioEngine, EngineConfig, EngineState};
use crate::audio::test_backend::{FakeBackend, FakeShared};
use crate::domain::sound::Sound;
use crate::infrastructure::media::decode::SymphoniaClipDecoder;
use crate::infrastructure::media::fs::OsMediaFs;
use crate::infrastructure::sqlite::SqliteSoundRepository;

const MONO_44K: OutputFormat = OutputFormat {
    sample_rate: 44_100,
    channels: 1,
};

struct CountingDecoder {
    inner: SymphoniaClipDecoder,
    calls: AtomicUsize,
}

impl ClipDecoder for CountingDecoder {
    fn decode(
        &self,
        file: File,
        target: OutputFormat,
        limits: &ProbeLimits,
        cancel: &CancelToken,
    ) -> Result<Arc<Clip>, ProbeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.decode(file, target, limits, cancel)
    }
}

/// Changes the device format in the middle of the first decode, as a device switch between preparing a clip and playing it.
struct SwitchingDecoder {
    inner: SymphoniaClipDecoder,
    shared: Arc<FakeShared>,
    engine: Arc<AudioEngine>,
    switched: AtomicUsize,
    calls: AtomicUsize,
}

impl ClipDecoder for SwitchingDecoder {
    fn decode(
        &self,
        file: File,
        target: OutputFormat,
        limits: &ProbeLimits,
        cancel: &CancelToken,
    ) -> Result<Arc<Clip>, ProbeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let clip = self.inner.decode(file, target, limits, cancel);
        if self.switched.fetch_add(1, Ordering::SeqCst) == 0 {
            self.shared.set_default_format(MONO_44K);
            self.shared.emit(StreamEvent::Invalidated);
            wait_for("the device to switch format", || {
                self.engine.snapshot().format == Some(MONO_44K)
            });
        }
        clip
    }
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

struct Harness {
    env: Env,
    shared: Arc<FakeShared>,
    engine: Arc<AudioEngine>,
}

impl Harness {
    fn new() -> Self {
        Self::with_plan(|_| {})
    }

    fn with_plan(prepare: impl FnOnce(&Arc<FakeShared>)) -> Self {
        let shared = FakeShared::new();
        prepare(&shared);
        let backend_shared = Arc::clone(&shared);
        let config = EngineConfig {
            poll_interval: Duration::from_millis(10),
            retry_delays: vec![Duration::from_millis(10), Duration::from_millis(20)],
        };
        let engine = Arc::new(
            AudioEngine::start(config, move || Box::new(FakeBackend(backend_shared)))
                .expect("engine"),
        );
        Self {
            env: Env::new(),
            shared,
            engine,
        }
    }

    fn wait_running(&self) {
        wait_for("the engine to run", || {
            self.engine.snapshot().state == EngineState::Running
        });
    }

    fn import(&self, fixture: &str) -> Sound {
        match self.env.import(&self.env.source(fixture, fixture)) {
            Ok(ImportOutcome::Imported(sound)) => sound,
            other => panic!("import failed: {other:?}"),
        }
    }

    fn service(&self, decoder: Arc<dyn ClipDecoder>, limits: PlaybackLimits) -> PlaybackService {
        let files = Arc::new(OsMediaFs::open(&self.env.root).expect("library dirs"));
        let repository = Arc::new(SqliteSoundRepository::new(Arc::clone(&self.env.database)));
        PlaybackService::new(repository, files, decoder, Arc::clone(&self.engine), limits)
    }

    fn counting(&self) -> Arc<CountingDecoder> {
        Arc::new(CountingDecoder {
            inner: SymphoniaClipDecoder,
            calls: AtomicUsize::new(0),
        })
    }

    fn pump_until_sound(&self) -> Vec<f32> {
        let mut heard = Vec::new();
        wait_for("sound from the stream", || {
            heard = self.shared.pump(256).unwrap_or_default();
            heard.iter().any(|s| s.abs() > 0.01)
        });
        heard
    }

    /// Makes the managed file writable so a test can tamper with it, as a user or a faulty disk might.
    fn tamper(&self, sound: &Sound, change: impl FnOnce(&std::path::Path)) {
        let path = self.env.media_path(sound.key.as_str());
        let mut permissions = fs::metadata(&path).expect("meta").permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).expect("permissions");
        change(&path);
    }
}

fn limits() -> PlaybackLimits {
    PlaybackLimits::provisional()
}

#[test]
fn an_imported_sound_is_decoded_and_heard_through_the_stream() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    let service = harness.service(harness.counting(), limits());

    service.play(sound.id).expect("play");
    let heard = harness.pump_until_sound();
    assert!(heard.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    assert!(
        heard
            .as_chunks::<2>()
            .0
            .iter()
            .any(|[left, right]| *left != 0.0 && left == right),
        "mono tone on both channels"
    );
}

#[test]
fn triggering_a_sound_again_does_not_decode_it_again() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.flac");
    let decoder = harness.counting();
    let service = harness.service(decoder.clone(), limits());

    for _ in 0..40 {
        service.play(sound.id).expect("play");
        harness.shared.pump(512);
    }
    assert_eq!(
        decoder.calls.load(Ordering::SeqCst),
        1,
        "decoded once, then served from the cache"
    );
}

#[test]
fn a_device_format_change_makes_the_next_play_decode_for_the_new_format() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    let decoder = harness.counting();
    let service = harness.service(decoder.clone(), limits());
    service.play(sound.id).expect("play on stereo");

    harness.shared.set_default_format(MONO_44K);
    harness.shared.emit(StreamEvent::Invalidated);
    wait_for("the new format", || {
        harness.engine.snapshot().format == Some(MONO_44K)
    });

    service.play(sound.id).expect("play on mono");
    assert_eq!(decoder.calls.load(Ordering::SeqCst), 2);
    let heard = harness.pump_until_sound();
    assert_eq!(heard.len(), 256, "one channel per frame");
}

#[test]
fn a_device_switch_between_decoding_and_playing_is_handled_by_decoding_again() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    let decoder = Arc::new(SwitchingDecoder {
        inner: SymphoniaClipDecoder,
        shared: Arc::clone(&harness.shared),
        engine: Arc::clone(&harness.engine),
        switched: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
    });
    let service = harness.service(decoder.clone(), limits());

    service
        .play(sound.id)
        .expect("the retry succeeds on the new format");
    assert_eq!(
        decoder.calls.load(Ordering::SeqCst),
        2,
        "first for stereo, then again for mono"
    );
    harness.pump_until_sound();
}

#[test]
fn an_unknown_sound_is_reported() {
    let harness = Harness::new();
    harness.wait_running();
    let service = harness.service(harness.counting(), limits());
    assert_eq!(service.play(424_242), Err(PlaybackError::SoundNotFound));
}

#[test]
fn a_missing_file_is_reported_and_nothing_is_played() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    harness.tamper(&sound, |path| fs::remove_file(path).expect("remove"));
    let service = harness.service(harness.counting(), limits());

    assert_eq!(service.play(sound.id), Err(PlaybackError::MediaMissing));
    assert_eq!(harness.shared.pump(64).expect("stream"), vec![0.0; 128]);
}

#[test]
fn a_file_changed_in_place_is_refused_by_its_hash_even_when_the_size_is_the_same() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    harness.tamper(&sound, |path| {
        let mut bytes = fs::read(path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x55;
        fs::write(path, bytes).expect("write");
    });
    let decoder = harness.counting();
    let service = harness.service(decoder.clone(), limits());

    assert_eq!(service.play(sound.id), Err(PlaybackError::MediaCorrupt));
    assert_eq!(
        decoder.calls.load(Ordering::SeqCst),
        0,
        "tampered bytes never reach the decoder"
    );
}

#[test]
fn a_file_with_a_different_size_is_refused() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    harness.tamper(&sound, |path| {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open")
            .set_len(100)
            .expect("truncate")
    });
    let service = harness.service(harness.counting(), limits());
    assert_eq!(service.play(sound.id), Err(PlaybackError::MediaCorrupt));
}

#[cfg(unix)]
#[test]
fn a_link_planted_where_a_managed_file_should_be_is_refused() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    let decoy = harness.env.source("tone.wav", "decoy.wav");
    harness.tamper(&sound, |path| {
        fs::remove_file(path).expect("remove");
        std::os::unix::fs::symlink(&decoy, path).expect("symlink");
    });
    let service = harness.service(harness.counting(), limits());
    assert_eq!(service.play(sound.id), Err(PlaybackError::MediaCorrupt));
}

#[test]
fn clip_length_and_decode_time_are_limited() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");

    let short = PlaybackLimits {
        max_clip_ms: 100,
        ..limits()
    };
    assert_eq!(
        harness.service(harness.counting(), short).play(sound.id),
        Err(PlaybackError::ClipTooLong)
    );

    let no_time = PlaybackLimits {
        max_decode_time: Duration::ZERO,
        ..limits()
    };
    std::thread::sleep(Duration::from_millis(2));
    assert_eq!(
        harness.service(harness.counting(), no_time).play(sound.id),
        Err(PlaybackError::DecodeTimeout)
    );
}

#[test]
fn without_a_running_output_play_reports_that_audio_is_unavailable() {
    let harness = Harness::with_plan(|shared| {
        shared.plan([Err(BackendError::Unsupported)]);
        shared.set_default_id(None);
    });
    wait_for("failed", || {
        matches!(harness.engine.snapshot().state, EngineState::Failed(_))
    });
    let sound = harness.import("tone.wav");
    let service = harness.service(harness.counting(), limits());
    assert_eq!(service.play(sound.id), Err(PlaybackError::AudioUnavailable));
    assert_eq!(
        service.stop_all(),
        Ok(()),
        "stopping with nothing running is harmless"
    );
}

#[test]
fn stop_all_silences_everything_that_is_playing() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.flac");
    let service = harness.service(harness.counting(), limits());
    service.play(sound.id).expect("play");
    harness.pump_until_sound();

    service.stop_all().expect("stop all");
    wait_for("silence", || {
        harness
            .shared
            .pump(256)
            .is_some_and(|out| out.iter().all(|s| *s == 0.0))
    });
}

#[test]
fn a_tiny_cache_budget_still_plays_but_decodes_every_time() {
    let harness = Harness::new();
    harness.wait_running();
    let sound = harness.import("tone.wav");
    let decoder = harness.counting();
    let service = harness.service(
        decoder.clone(),
        PlaybackLimits {
            cache_bytes: 16,
            ..limits()
        },
    );
    for _ in 0..3 {
        service.play(sound.id).expect("play");
        harness.shared.pump(512);
    }
    assert_eq!(decoder.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn many_threads_can_trigger_sounds_at_once() {
    let harness = Harness::new();
    harness.wait_running();
    let first = harness.import("tone.wav");
    let second = harness.import("tone.flac");
    let service = Arc::new(harness.service(harness.counting(), limits()));

    let workers: Vec<_> = (0..6)
        .map(|n| {
            let service = Arc::clone(&service);
            let id = if n % 2 == 0 { first.id } else { second.id };
            std::thread::spawn(move || (0..10).map(|_| service.play(id)).collect::<Vec<_>>())
        })
        .collect();
    for worker in workers {
        for result in worker.join().expect("worker") {
            assert!(
                result.is_ok() || result == Err(PlaybackError::AudioBusy),
                "{result:?}"
            );
        }
    }
    harness.pump_until_sound();
}

#[test]
fn importing_and_playing_share_the_same_library_without_interfering() {
    // The import service of the harness and the playback service use the same database and files.
    let harness = Harness::new();
    harness.wait_running();
    let service = harness.service(harness.counting(), limits());
    let sound = harness.import("tone.ogg");
    service.play(sound.id).expect("play right after the import");
    harness.pump_until_sound();
    harness.env.assert_consistent();
}
