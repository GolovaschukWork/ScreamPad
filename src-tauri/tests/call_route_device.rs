//! Opt-in hardware run of the production engine. No microphone samples are saved.

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires an explicitly selected microphone and virtual output on macOS"]
fn production_route_runs_for_ten_minutes_with_bounded_queues() {
    use std::thread;
    use std::time::Duration;

    use soundboard_desktop_lib::audio::backend::RouteDevices;
    use soundboard_desktop_lib::audio::engine::{AudioEngine, EngineConfig, RouteState};
    use soundboard_desktop_lib::infrastructure::audio::cpal_backend::CpalBackend;

    let microphone_id = std::env::var("SCREAMPAD_TEST_MIC_ID").expect("explicit microphone ID");
    let virtual_output_id =
        std::env::var("SCREAMPAD_TEST_VIRTUAL_ID").expect("explicit virtual output ID");
    let engine = AudioEngine::start(EngineConfig::production(), || Box::new(CpalBackend::new()))
        .expect("audio engine thread");
    engine
        .start_route(RouteDevices {
            microphone_id,
            virtual_output_id,
        })
        .expect("start routing");
    for minute in 1..=10 {
        thread::sleep(Duration::from_secs(60));
        let snapshot = engine.route_snapshot();
        assert_eq!(
            snapshot.state,
            RouteState::Running,
            "route failed at minute {minute}"
        );
        let counters = snapshot.counters.expect("live metrics");
        let expected_input_frames = counters.rendered as f64 * f64::from(counters.input_rate)
            / f64::from(counters.output_rate);
        let clock_delta = (counters.captured + counters.overflow) as f64 - expected_input_frames;
        println!(
            "minute={minute} input_rate={} output_rate={} input_buffer={} output_buffer={} capacity={} occupancy={} max_occupancy={} captured={} rendered={} overflow={} underflow={} discarded={} clipped={} xruns={} clock_delta_input_frames={clock_delta:.1}",
            counters.input_rate,
            counters.output_rate,
            counters.input_buffer_frames,
            counters.output_buffer_frames,
            counters.queue_capacity,
            counters.occupancy,
            counters.max_occupancy,
            counters.captured,
            counters.rendered,
            counters.overflow,
            counters.underflow,
            counters.discarded,
            counters.clipped,
            counters.xruns
        );
        assert!(counters.occupancy <= u64::from(counters.queue_capacity));
        assert!(counters.max_occupancy <= u64::from(counters.queue_capacity));
    }
    let final_status = engine.route_snapshot();
    let counters = final_status.counters.expect("final metrics");
    assert!(counters.captured > 0 && counters.rendered > 0);
    engine.stop_route().expect("stop capture");
    assert_eq!(engine.route_snapshot().state, RouteState::Stopped);
    engine.shutdown();
}

/// Opt-in hardware fan-out of an imported library file through the production CPAL route.
/// Numeric callback meters prove delivery into both mixers; they do not prove Discord audibility.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires a selected microphone, virtual output, and local output"]
fn imported_library_clip_reaches_local_and_virtual_callbacks() {
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use soundboard_desktop_lib::application::cancel::CancelToken;
    use soundboard_desktop_lib::application::import::ImportOutcome;
    use soundboard_desktop_lib::application::playback::{PlaybackLimits, PlaybackService};
    use soundboard_desktop_lib::audio::backend::RouteDevices;
    use soundboard_desktop_lib::audio::engine::{
        AudioEngine, EngineConfig, EngineState, RouteState,
    };
    use soundboard_desktop_lib::infrastructure::audio::cpal_backend::CpalBackend;
    use soundboard_desktop_lib::infrastructure::media::decode::SymphoniaClipDecoder;
    use soundboard_desktop_lib::infrastructure::sqlite::{Database, migrations};
    use soundboard_desktop_lib::state::StorageState;

    let microphone_id = std::env::var("SCREAMPAD_TEST_MIC_ID").expect("explicit microphone ID");
    let virtual_output_id =
        std::env::var("SCREAMPAD_TEST_VIRTUAL_ID").expect("explicit virtual output ID");
    let local_output_id =
        std::env::var("SCREAMPAD_TEST_LOCAL_ID").expect("explicit local output ID");
    let dir = std::env::temp_dir().join(format!("screampad-route-clip-{}", std::process::id()));
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
            .expect("audio engine"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.snapshot().state != EngineState::Running {
        assert!(Instant::now() < deadline, "local output did not start");
        thread::sleep(Duration::from_millis(20));
    }
    engine
        .set_local_output(Some(local_output_id))
        .expect("local output");
    engine
        .start_route(RouteDevices {
            microphone_id,
            virtual_output_id,
        })
        .expect("route");
    let service = PlaybackService::new(
        library.repository.clone(),
        library.files.clone(),
        Arc::new(SymphoniaClipDecoder),
        Arc::clone(&engine),
        PlaybackLimits::provisional(),
    );
    let local_before = engine.snapshot().callbacks;
    service.play(sound.id).expect("real library play");
    thread::sleep(Duration::from_millis(500));
    let local_after = engine.snapshot();
    let route = engine.route_snapshot();
    let counters = route.counters.expect("route counters");
    println!(
        "local_callbacks={} route_clip_energy={} route_mic_energy={} route_rendered={} clip_errors={}",
        local_after.callbacks - local_before,
        counters.clip_energy,
        counters.mic_energy,
        counters.rendered,
        counters.clip_errors
    );
    assert_eq!(route.state, RouteState::Running);
    assert!(local_after.callbacks > local_before);
    assert!(
        counters.clip_energy > 0,
        "imported clip did not reach virtual callback"
    );
    assert_eq!(counters.clip_errors, 0);
    service.stop(sound.id).expect("stop both directions");
    engine.stop_route().expect("stop capture");
    assert_eq!(engine.route_snapshot().state, RouteState::Stopped);
    assert_eq!(engine.snapshot().state, EngineState::Running);
    engine.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}
