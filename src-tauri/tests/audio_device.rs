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
