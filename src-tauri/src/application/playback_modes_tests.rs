//! Gain and play modes (ADR 0010): what a trigger does depends on the sound's stored options and on what is already playing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::playback::{PlayOutcome, PlaybackService};
use super::ports::{ClipDecoder, SoundRepository};
use super::test_env::Env;
use crate::audio::backend::StreamEvent;
use crate::audio::engine::{AudioEngine, EngineConfig, EngineState};
use crate::audio::mixer::MAX_VOICES;
use crate::audio::test_backend::{FakeBackend, FakeShared};
use crate::domain::playback::{PlayMode, SoundGain};
use crate::domain::sound::Sound;
use crate::infrastructure::media::decode::SymphoniaClipDecoder;
use crate::infrastructure::media::fs::OsMediaFs;
use crate::infrastructure::sqlite::SqliteSoundRepository;

use super::playback::PlaybackLimits;

/// Frames of the 48 kHz stream that cover the 5 ms fade of a stopped voice with room to spare.
const FLUSH_FRAMES: usize = 2400;

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

struct Rig {
    env: Env,
    shared: Arc<FakeShared>,
    engine: Arc<AudioEngine>,
    repository: Arc<SqliteSoundRepository>,
    service: Arc<PlaybackService>,
}

impl Rig {
    fn new() -> Self {
        let shared = FakeShared::new();
        let backend_shared = Arc::clone(&shared);
        let config = EngineConfig {
            poll_interval: Duration::from_millis(10),
            retry_delays: vec![Duration::from_millis(10)],
        };
        let engine = Arc::new(
            AudioEngine::start(config, move || Box::new(FakeBackend(backend_shared)))
                .expect("engine"),
        );
        wait_for("the engine to run", || {
            engine.snapshot().state == EngineState::Running
        });
        let env = Env::new();
        let repository = Arc::new(SqliteSoundRepository::new(Arc::clone(&env.database)));
        let service = Arc::new(PlaybackService::new(
            repository.clone(),
            Arc::new(OsMediaFs::open(&env.root).expect("library dirs")),
            Arc::new(SymphoniaClipDecoder) as Arc<dyn ClipDecoder>,
            Arc::clone(&engine),
            PlaybackLimits::provisional(),
        ));
        Self {
            env,
            shared,
            engine,
            repository,
            service,
        }
    }

    fn import(&self, fixture: &str) -> Sound {
        match self.env.import(&self.env.source(fixture, fixture)) {
            Ok(super::import::ImportOutcome::Imported(sound)) => sound,
            other => panic!("import failed: {other:?}"),
        }
    }

    fn options(&self, sound: &Sound, gain_millibels: i32, mode: PlayMode) {
        self.repository
            .set_options(
                sound.id,
                SoundGain::from_millibels(gain_millibels).expect("gain"),
                mode,
                0,
            )
            .expect("set options")
            .expect("exists");
    }

    fn pump(&self, frames: usize) -> Vec<f32> {
        self.shared.pump(frames).expect("a stream is open")
    }

    /// Stops everything and lets the fade finish, so the next trigger starts from silence.
    fn silence(&self) {
        self.service.stop_all().expect("stop all");
        self.pump(FLUSH_FRAMES);
        assert!(self.pump(64).iter().all(|s| *s == 0.0), "silence");
    }

    /// What a lone voice of `sound` sounds like in frames `skip..skip + take` of the stream.
    fn reference(&self, sound: &Sound, skip: usize, take: usize) -> Vec<f32> {
        self.silence();
        let mode = sound.play_mode;
        self.options(sound, 0, PlayMode::Overlap);
        assert_eq!(self.service.play(sound.id), Ok(PlayOutcome::Started));
        self.pump(skip);
        let heard = self.pump(take);
        self.silence();
        self.options(sound, sound.gain.millibels(), mode);
        heard
    }
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
}

fn assert_close(actual: &[f32], expected: &[f32], what: &str) {
    assert_eq!(actual.len(), expected.len());
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).abs() < 1e-4, "{what}: sample {index}: {a} vs {e}");
    }
}

#[test]
fn a_sounds_gain_scales_what_is_heard_by_db_over_20() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    let unity = rig.reference(&sound, 200, 64);
    assert!(peak(&unity) > 0.05, "the reference is audible");

    for (millibels, factor) in [(-2000, 0.1_f32), (-600, 0.501_187_2), (600, 1.995_262_3)] {
        rig.options(&sound, millibels, PlayMode::Overlap);
        rig.silence();
        assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
        rig.pump(200);
        let heard = rig.pump(64);
        let expected: Vec<f32> = unity
            .iter()
            .map(|s| (s * factor).clamp(-1.0, 1.0))
            .collect();
        assert_close(&heard, &expected, &format!("{millibels} mB"));
    }
}

#[test]
fn the_master_gain_scales_everything_and_survives_a_device_change() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    let unity = rig.reference(&sound, 200, 64);

    rig.service
        .apply_master_gain(
            crate::domain::playback::MasterGain::from_millibels(-2000).expect("gain"),
        )
        .expect("master gain");
    rig.service.play(sound.id).expect("play");
    rig.pump(200);
    let expected: Vec<f32> = unity.iter().map(|s| s * 0.1).collect();
    assert_close(&rig.pump(64), &expected, "master -20 dB");

    // The stream is rebuilt (a new mixer starts at unity); the master gain the user chose must still apply.
    rig.shared.emit(StreamEvent::Invalidated);
    wait_for("the stream to be rebuilt", || {
        rig.engine.snapshot().opens == 2
    });
    rig.service.play(sound.id).expect("play on the new stream");
    rig.pump(200);
    assert_close(&rig.pump(64), &expected, "master -20 dB after the rebuild");
}

#[test]
fn overlap_starts_another_voice_each_time() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    let single = rig.reference(&sound, 200, 64);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    rig.pump(200);
    let doubled: Vec<f32> = single.iter().map(|s| (s * 2.0).clamp(-1.0, 1.0)).collect();
    assert_close(&rig.pump(64), &doubled, "two voices sum");
}

#[test]
fn ignore_does_nothing_while_the_sound_plays_and_starts_again_when_it_is_over() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    let single = rig.reference(&sound, 200, 64);
    rig.options(&sound, 0, PlayMode::Ignore);

    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Ignored));
    rig.pump(200);
    assert_close(&rig.pump(64), &single, "a single voice");

    // The 250 ms clip ends by itself (12 000 frames at 48 kHz).
    rig.pump(13_000);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
}

#[test]
fn toggle_stops_a_playing_sound_and_starts_a_silent_one() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.options(&sound, 0, PlayMode::Toggle);

    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    rig.pump(200);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Stopped));
    rig.pump(FLUSH_FRAMES);
    assert!(rig.pump(256).iter().all(|s| *s == 0.0), "it fell silent");
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    assert!(peak(&rig.pump(512)) > 0.0);
}

#[test]
fn a_voice_that_is_fading_out_no_longer_counts_as_playing() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.options(&sound, 0, PlayMode::Toggle);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    rig.pump(200);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Stopped));
    // No audio callback has run since: the stopped voice still holds its slot, but a third trigger must start, not stop again.
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
}

#[test]
fn restart_replaces_the_playing_voice_with_one_from_the_beginning() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    let single = rig.reference(&sound, FLUSH_FRAMES + 200, 64);
    rig.options(&sound, 0, PlayMode::Restart);

    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    rig.pump(5_000);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Restarted));
    // The old voice fades out within 5 ms; afterwards only the new voice, now FLUSH_FRAMES + 200 frames in, is heard.
    rig.pump(FLUSH_FRAMES + 200);
    assert_close(&rig.pump(64), &single, "only the restarted voice");
}

#[test]
fn a_mode_only_affects_its_own_sound() {
    let rig = Rig::new();
    let first = rig.import("tone.wav");
    let second = rig.import("tone.flac");
    rig.options(&first, 0, PlayMode::Toggle);
    rig.options(&second, 0, PlayMode::Toggle);

    assert_eq!(rig.service.play(first.id), Ok(PlayOutcome::Started));
    assert_eq!(rig.service.play(second.id), Ok(PlayOutcome::Started));
    assert_eq!(rig.service.play(first.id), Ok(PlayOutcome::Stopped));
    rig.pump(FLUSH_FRAMES);
    assert!(peak(&rig.pump(256)) > 0.0, "the other sound keeps playing");
    assert_eq!(rig.service.play(second.id), Ok(PlayOutcome::Stopped));
}

#[test]
fn stop_all_resets_the_bookkeeping_of_every_sound() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.options(&sound, 0, PlayMode::Toggle);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    rig.silence();
    // After Stop all nothing is playing, so a toggle starts instead of stopping.
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
}

#[test]
fn a_rebuilt_stream_has_no_voices_so_a_toggle_starts() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.options(&sound, 0, PlayMode::Toggle);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    rig.shared.emit(StreamEvent::Invalidated);
    wait_for("the stream to be rebuilt", || {
        rig.engine.snapshot().opens == 2
    });
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
}

#[test]
fn simultaneous_triggers_of_an_ignore_sound_start_exactly_one_voice() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.options(&sound, 0, PlayMode::Ignore);
    // Decode once up front so every thread races on the decision, not on the first decode.
    rig.service.play(sound.id).expect("warm up");

    for round in 0..40 {
        rig.silence();
        let start = Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let service = Arc::clone(&rig.service);
                let start = Arc::clone(&start);
                let id = sound.id;
                std::thread::spawn(move || {
                    start.wait();
                    service.play(id)
                })
            })
            .collect();
        let outcomes: Vec<_> = workers
            .into_iter()
            .map(|w| w.join().expect("worker").expect("play"))
            .collect();
        let started = outcomes
            .iter()
            .filter(|o| **o == PlayOutcome::Started)
            .count();
        assert_eq!(started, 1, "round {round}: {outcomes:?}");
    }
}

#[test]
fn beyond_the_voice_limit_a_play_is_refused_and_nothing_else_is_stopped() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    let extra = 8;
    for _ in 0..MAX_VOICES + extra {
        assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    }
    rig.pump(64);
    wait_for("the refused plays to be counted", || {
        rig.engine.snapshot().rejected_plays == extra as u64
    });
    // The 32 voices that fit keep sounding.
    assert!(peak(&rig.pump(256)) > 0.0);
}

#[test]
fn an_unknown_sound_cannot_be_triggered() {
    let rig = Rig::new();
    assert_eq!(
        rig.service.play(424_242),
        Err(super::playback::PlaybackError::SoundNotFound)
    );
}

// --- stopping one sound and reporting what plays ------------------------------------------------------------------------

#[test]
fn stopping_one_sound_silences_only_that_sound() {
    let rig = Rig::new();
    let first = rig.import("tone.wav");
    let second = rig.import("tone.flac");
    rig.service.play(first.id).expect("first");
    rig.service.play(second.id).expect("second");
    assert_eq!(rig.service.playing_sounds(), Ok(vec![first.id, second.id]));

    rig.service.stop(first.id).expect("stop");
    assert_eq!(rig.service.playing_sounds(), Ok(vec![second.id]));
    rig.pump(FLUSH_FRAMES);
    assert!(peak(&rig.pump(256)) > 0.0, "the other sound keeps playing");
}

#[test]
fn stop_ends_every_overlapping_voice_of_the_sound() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    for _ in 0..3 {
        assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    }
    rig.service.stop(sound.id).expect("stop");
    rig.pump(FLUSH_FRAMES);
    assert!(
        rig.pump(256).iter().all(|s| *s == 0.0),
        "all three voices are gone"
    );
    assert_eq!(rig.service.playing_sounds(), Ok(Vec::new()));
}

#[test]
fn stopping_a_sound_that_is_not_playing_or_does_not_exist_is_not_an_error() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    assert_eq!(rig.service.stop(sound.id), Ok(()));
    assert_eq!(rig.service.stop(424_242), Ok(()));
    assert_eq!(rig.service.stop(-1), Ok(()));
}

#[test]
fn a_sound_stops_being_reported_when_it_ends_by_itself() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.service.play(sound.id).expect("play");
    assert_eq!(rig.service.playing_sounds(), Ok(vec![sound.id]));
    // The 250 ms clip is 12 000 frames at 48 kHz.
    rig.pump(13_000);
    wait_for("the end to be seen", || {
        rig.service.playing_sounds() == Ok(Vec::new())
    });
}

#[test]
fn stop_after_toggle_style_play_still_starts_again_afterwards() {
    let rig = Rig::new();
    let sound = rig.import("tone.wav");
    rig.options(&sound, 0, PlayMode::Ignore);
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Ignored));
    rig.service.stop(sound.id).expect("stop");
    assert_eq!(rig.service.play(sound.id), Ok(PlayOutcome::Started));
}
