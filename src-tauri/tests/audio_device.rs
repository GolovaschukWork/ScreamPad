//! Runs the engine against the real default audio output. It needs audio hardware, so it is ignored by default (CI machines
//! have none). Run it by hand on a machine with a speaker or headphones:
//!
//!     cargo test --test audio_device -- --ignored --nocapture
//!
//! It plays **silence**, so nothing is audible, but it proves that the device pulls audio through the real callback.

use std::sync::Arc;
use std::time::{Duration, Instant};

use soundboard_desktop_lib::audio::clip::Clip;
use soundboard_desktop_lib::audio::engine::{AudioEngine, EngineConfig, EngineState};
use soundboard_desktop_lib::infrastructure::audio::cpal_backend::CpalBackend;

#[test]
#[ignore = "needs an audio output device"]
fn the_real_default_output_pulls_audio_through_the_engine() {
    let engine = AudioEngine::start(EngineConfig::production(), || Box::new(CpalBackend::new()))
        .expect("engine thread");

    let deadline = Instant::now() + Duration::from_secs(10);
    let format = loop {
        let snapshot = engine.snapshot();
        match snapshot.state {
            EngineState::Running => break snapshot.format.expect("a running stream has a format"),
            EngineState::Failed(reason) => {
                panic!("the engine failed to open the default output: {reason:?}")
            }
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "no output stream within 10 s: {snapshot:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    println!(
        "default output: {} Hz, {} channels",
        format.sample_rate, format.channels
    );

    // 0.3 s of silence in the device's own format.
    let frames = (format.sample_rate as usize * 3) / 10;
    let silence = Clip::new(
        vec![0.0; frames * usize::from(format.channels)],
        format.channels,
        format.sample_rate,
    )
    .expect("clip");
    engine.play(Arc::clone(&silence), 1.0).expect("play");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Arc::strong_count(&silence) > 1 {
        assert!(
            Instant::now() < deadline,
            "the clip never finished: {:?}",
            engine.snapshot()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let snapshot = engine.snapshot();
    println!("after playback: {snapshot:?}");
    assert!(
        snapshot.callbacks > 0,
        "the device never called the audio callback"
    );
    assert_eq!(snapshot.state, EngineState::Running);

    engine.shutdown();
    assert_eq!(engine.snapshot().state, EngineState::Stopped);
}

/// The whole path on real hardware: import a file, then play it through the real output. The gain is zero, so nothing is
/// audible, but the file is hashed, decoded, resampled to the device rate and mixed by the real callback.
#[test]
#[ignore = "needs an audio output device"]
fn an_imported_sound_plays_through_the_real_output_silently() {
    use soundboard_desktop_lib::application::cancel::CancelToken;
    use soundboard_desktop_lib::application::import::ImportOutcome;
    use soundboard_desktop_lib::application::playback::{PlaybackLimits, PlaybackService};
    use soundboard_desktop_lib::infrastructure::media::decode::SymphoniaClipDecoder;
    use soundboard_desktop_lib::infrastructure::sqlite::{Database, migrations};
    use soundboard_desktop_lib::state::StorageState;

    let dir = std::env::temp_dir().join(format!("screampad-device-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let database = Database::open(&dir.join("library.db"), &migrations()).expect("database");
    let storage = StorageState::ready(Arc::new(database), &dir.join("library"));
    let library = storage.library().expect("library");

    let fixture = format!(
        "{}/tests/fixtures/audio/tone.mp3",
        env!("CARGO_MANIFEST_DIR")
    );
    let sound = match library
        .importer
        .import(std::path::Path::new(&fixture), &CancelToken::new())
    {
        Ok(ImportOutcome::Imported(sound)) => sound,
        other => panic!("import failed: {other:?}"),
    };

    let engine = Arc::new(
        AudioEngine::start(EngineConfig::production(), || Box::new(CpalBackend::new()))
            .expect("engine"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.snapshot().state != EngineState::Running {
        assert!(
            Instant::now() < deadline,
            "no output stream: {:?}",
            engine.snapshot()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let service = PlaybackService::new(
        library.repository.clone(),
        library.files.clone(),
        Arc::new(SymphoniaClipDecoder),
        Arc::clone(&engine),
        PlaybackLimits::provisional(),
    );

    // Silent test: the engine's raw master gain is set to zero (the persisted setting cannot be below -60 dB).
    engine.set_master_gain(0.0).expect("mute the output");

    // A 44.1 kHz or 8 kHz file on a 48 kHz device: the resampler is on the path.
    service.play(sound.id).expect("play");
    service.play(sound.id).expect("play again from the cache");
    std::thread::sleep(Duration::from_millis(700));

    let snapshot = engine.snapshot();
    println!("after playing: {snapshot:?}");
    assert!(snapshot.callbacks > 0);
    assert_eq!(snapshot.rejected_plays, 0);
    assert_eq!(snapshot.state, EngineState::Running);
    engine.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}
