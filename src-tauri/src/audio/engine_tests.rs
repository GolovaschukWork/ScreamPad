//! Engine tests against a fake audio API: the device state machine, recovery and shutdown.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::backend::{BackendError, OutputFormat, StreamEvent};
use super::clip::Clip;
use super::engine::{AudioEngine, EngineConfig, EngineError, EngineState, FailureReason};
use super::test_backend::{FakeBackend, FakeShared, STEREO_48K};

const MONO_44K: OutputFormat = OutputFormat {
    sample_rate: 44_100,
    channels: 1,
};

fn fast_config() -> EngineConfig {
    EngineConfig {
        poll_interval: Duration::from_millis(10),
        retry_delays: vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
        ],
    }
}

fn engine(shared: &Arc<FakeShared>) -> AudioEngine {
    let shared = Arc::clone(shared);
    AudioEngine::start(fast_config(), move || Box::new(FakeBackend(shared))).expect("engine thread")
}

fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_running(engine: &AudioEngine) {
    wait_for("the engine to run", || {
        engine.snapshot().state == EngineState::Running
    });
}

fn clip(value: f32, frames: usize, format: OutputFormat) -> Arc<Clip> {
    Clip::new(
        vec![value; frames * usize::from(format.channels)],
        format.channels,
        format.sample_rate,
    )
    .expect("clip")
}

#[test]
fn the_engine_opens_the_default_output_and_reports_its_format() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.format, Some(STEREO_48K));
    assert_eq!((snapshot.opens, snapshot.device_losses), (1, 0));
    assert_eq!(shared.streams_alive(), 1);
}

#[test]
fn a_played_clip_is_audible_through_the_stream_and_stop_all_silences_it() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    engine
        .play(clip(0.25, 48_000, STEREO_48K), 1.0)
        .expect("play");
    wait_for("the clip to sound", || {
        shared
            .pump(4)
            .is_some_and(|out| out.iter().all(|s| *s == 0.25))
    });

    engine.stop_all().expect("stop all");
    wait_for("silence after stop_all", || {
        shared
            .pump(4)
            .is_some_and(|out| out.iter().all(|s| *s == 0.0))
    });
}

#[test]
fn a_clip_for_another_format_is_refused_so_the_caller_can_decode_it_again() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    assert_eq!(
        engine.play(clip(0.1, 100, MONO_44K), 1.0),
        Err(EngineError::FormatMismatch)
    );
    let same_channels_other_rate = OutputFormat {
        sample_rate: 44_100,
        channels: 2,
    };
    assert_eq!(
        engine.play(clip(0.1, 100, same_channels_other_rate), 1.0),
        Err(EngineError::FormatMismatch)
    );
}

#[test]
fn a_lost_device_is_recovered_and_the_old_voices_are_dropped() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    engine
        .play(clip(0.25, 480_000, STEREO_48K), 1.0)
        .expect("play");
    wait_for("the clip to sound", || {
        shared.pump(2).is_some_and(|out| out[0] == 0.25)
    });

    shared.emit(StreamEvent::DeviceLost);
    wait_for("a second stream", || {
        engine.snapshot().opens == 2 && engine.snapshot().state == EngineState::Running
    });

    assert_eq!(engine.snapshot().device_losses, 1);
    assert_eq!(shared.streams_alive(), 1, "the lost stream was released");
    assert_eq!(
        shared.pump(4).expect("new stream"),
        vec![0.0; 8],
        "playback does not resume by itself"
    );
    engine
        .play(clip(0.5, 100, STEREO_48K), 1.0)
        .expect("the new stream accepts clips");
}

#[test]
fn recovery_adapts_to_a_device_with_a_different_format() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    let stereo_clip = clip(0.2, 100, STEREO_48K);

    shared.set_default_format(MONO_44K);
    shared.emit(StreamEvent::Invalidated);
    wait_for("the new format", || {
        engine.snapshot().format == Some(MONO_44K)
    });

    assert_eq!(
        engine.play(stereo_clip, 1.0),
        Err(EngineError::FormatMismatch),
        "old clips must be decoded again"
    );
    engine
        .play(clip(0.2, 100, MONO_44K), 1.0)
        .expect("a clip in the new format plays");
}

#[test]
fn a_busy_device_is_retried_with_backoff_until_it_opens() {
    let shared = FakeShared::new();
    shared.plan([Err(BackendError::Busy), Err(BackendError::NoDevice)]);
    let engine = engine(&shared);

    wait_running(&engine);
    assert_eq!(shared.open_calls(), 3, "two failures, then success");
    assert_eq!(engine.snapshot().opens, 1);
}

#[test]
fn a_recovering_engine_refuses_to_play() {
    let shared = FakeShared::new();
    shared.plan([
        Err(BackendError::Busy),
        Err(BackendError::Busy),
        Err(BackendError::Busy),
    ]);
    let engine = engine(&shared);
    wait_for("recovering", || {
        matches!(engine.snapshot().state, EngineState::Recovering { .. })
    });
    assert_eq!(
        engine.play(clip(0.1, 10, STEREO_48K), 1.0),
        Err(EngineError::Unavailable)
    );
}

#[test]
fn giving_up_ends_in_failed_and_a_retry_recovers() {
    let shared = FakeShared::new();
    // One initial attempt plus one per retry delay, all failing.
    shared.plan(std::iter::repeat_n(Err(BackendError::NoDevice), 4));
    shared.set_default_id(None);
    let engine = engine(&shared);

    wait_for("failed", || {
        engine.snapshot().state == EngineState::Failed(FailureReason::NoDevice)
    });
    assert_eq!(shared.open_calls(), 4);
    assert_eq!(
        engine.play(clip(0.1, 10, STEREO_48K), 1.0),
        Err(EngineError::Unavailable)
    );

    // The plan is used up, so the next open succeeds.
    engine.retry().expect("retry");
    wait_running(&engine);
}

#[test]
fn a_failure_that_cannot_be_retried_fails_at_once() {
    for (error, reason) in [
        (BackendError::Unsupported, FailureReason::Unsupported),
        (
            BackendError::PermissionDenied,
            FailureReason::PermissionDenied,
        ),
        (BackendError::Other, FailureReason::Other),
    ] {
        let shared = FakeShared::new();
        shared.plan([Err(error)]);
        let engine = engine(&shared);
        wait_for("failed", || {
            engine.snapshot().state == EngineState::Failed(reason)
        });
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(shared.open_calls(), 1, "{error:?} must not be retried");
    }
}

#[test]
fn a_new_default_device_is_noticed_by_polling_and_the_stream_moves_to_it() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    shared.set_default_format(MONO_44K);
    shared.set_default_id(Some("headphones"));
    wait_for("the stream to move", || {
        engine.snapshot().format == Some(MONO_44K)
    });
    assert_eq!(shared.streams_alive(), 1);
    assert_eq!(engine.snapshot().opens, 2);
}

#[test]
fn a_default_change_event_triggers_the_check_immediately() {
    let shared = FakeShared::new();
    let engine = AudioEngine::start(
        EngineConfig {
            poll_interval: Duration::from_secs(3600),
            retry_delays: vec![Duration::from_millis(10)],
        },
        {
            let shared = Arc::clone(&shared);
            move || Box::new(FakeBackend(shared))
        },
    )
    .expect("engine");
    wait_running(&engine);

    shared.set_default_id(Some("usb-dac"));
    shared.emit(StreamEvent::DefaultChanged);
    wait_for("the rebuild", || engine.snapshot().opens == 2);
}

#[test]
fn a_device_that_vanishes_with_no_replacement_ends_in_failed_and_comes_back_when_one_appears() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    shared.set_default_id(None);
    shared.plan(std::iter::repeat_n(Err(BackendError::NoDevice), 8));
    wait_for("failed", || {
        matches!(
            engine.snapshot().state,
            EngineState::Failed(FailureReason::NoDevice)
        )
    });

    shared.plan([]);
    // The plan above held 8 failures; clear what is left, then plug a device in.
    while shared.plan.lock().expect("plan").pop_front().is_some() {}
    shared.set_default_id(Some("plugged-in"));
    wait_running(&engine);
}

#[test]
fn xruns_are_counted_and_do_not_disturb_playback() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    for _ in 0..5 {
        shared.emit(StreamEvent::Xrun);
    }
    wait_for("xruns counted", || engine.snapshot().xruns == 5);
    assert_eq!(engine.snapshot().state, EngineState::Running);
    assert_eq!(shared.open_calls(), 1);
}

#[test]
fn a_burst_of_errors_from_one_stream_causes_one_rebuild() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    // The error callback of the stream that fails keeps firing; all of them belong to that one stream.
    let failing_stream = shared.sink().expect("sink");
    for _ in 0..50 {
        failing_stream(StreamEvent::DeviceLost);
        failing_stream(StreamEvent::Failed);
    }
    wait_for("running again", || {
        engine.snapshot().opens >= 2 && engine.snapshot().state == EngineState::Running
    });
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(shared.open_calls(), 2, "one rebuild for the whole burst");
}

#[test]
fn late_errors_of_a_replaced_stream_are_ignored() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    let old_stream = shared.sink().expect("sink");
    old_stream(StreamEvent::DeviceLost);
    wait_for("the replacement", || {
        engine.snapshot().opens == 2 && engine.snapshot().state == EngineState::Running
    });

    // The old stream's callback fires again after its replacement is running: it must not tear the new stream down.
    for _ in 0..20 {
        old_stream(StreamEvent::DeviceLost);
        old_stream(StreamEvent::Invalidated);
    }
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(engine.snapshot().state, EngineState::Running);
    assert_eq!(shared.open_calls(), 2);
    assert_eq!(engine.snapshot().device_losses, 1);
}

#[test]
fn finished_clips_are_released_by_the_engine_thread() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    let played = clip(0.1, 8, STEREO_48K);
    engine.play(Arc::clone(&played), 1.0).expect("play");
    wait_for("the clip to be mixed", || {
        shared
            .pump(16)
            .is_some_and(|out| out[0] == 0.1 || Arc::strong_count(&played) <= 2)
    });
    // Pump until the voice ended, then the engine's periodic drain must release its reference.
    shared.pump(64);
    wait_for("the engine to release the clip", || {
        Arc::strong_count(&played) == 1
    });
}

#[test]
fn shutdown_stops_the_stream_and_later_calls_report_stopped() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);

    engine.shutdown();
    assert_eq!(shared.streams_alive(), 0, "the stream is released");
    assert_eq!(engine.snapshot().state, EngineState::Stopped);
    assert_eq!(
        engine.play(clip(0.1, 4, STEREO_48K), 1.0),
        Err(EngineError::Stopped)
    );
    engine.shutdown();
}

#[test]
fn dropping_the_engine_stops_the_stream() {
    let shared = FakeShared::new();
    {
        let engine = engine(&shared);
        wait_running(&engine);
        assert_eq!(shared.streams_alive(), 1);
    }
    assert_eq!(shared.streams_alive(), 0);
}

// --- playing tags and change notifications ------------------------------------------------------------------------------

use std::sync::Mutex;

/// Records every set the engine reports to a listener.
fn recorder(engine: &AudioEngine) -> Arc<Mutex<Vec<Vec<u64>>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    engine
        .on_playing_changed(move |tags| sink.lock().expect("recorder").push(tags.to_vec()))
        .expect("listener");
    seen
}

fn reported(seen: &Arc<Mutex<Vec<Vec<u64>>>>) -> Vec<Vec<u64>> {
    seen.lock().expect("recorder").clone()
}

#[test]
fn a_new_listener_is_told_the_current_state_at_once() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    let seen = recorder(&engine);
    assert_eq!(reported(&seen), vec![Vec::<u64>::new()]);
}

#[test]
fn playing_tags_are_sorted_deduplicated_and_ignore_untagged_voices() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    for tag in [7, 3, 7] {
        engine
            .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, tag)
            .expect("play");
    }
    engine
        .play(clip(0.1, 48_000, STEREO_48K), 1.0)
        .expect("untagged");
    assert_eq!(engine.playing_tags(), Ok(vec![3, 7]));
}

#[test]
fn the_listener_hears_a_start_a_stop_and_a_natural_end_exactly_once_each() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    let seen = recorder(&engine);

    engine
        .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, 5)
        .expect("play");
    engine
        .play_tagged(clip(0.1, 480, STEREO_48K), 1.0, 9)
        .expect("short");
    wait_for("both to be reported", || {
        reported(&seen).contains(&vec![5, 9])
    });

    // The short clip (480 frames) ends by itself once the callback has run enough.
    shared.pump(2_000);
    wait_for("the natural end", || {
        reported(&seen).last() == Some(&vec![5])
    });

    engine.stop_tag(5).expect("stop");
    wait_for("the stop", || reported(&seen).last() == Some(&Vec::new()));

    let history = reported(&seen);
    assert_eq!(
        history,
        vec![Vec::new(), vec![5], vec![5, 9], vec![5], Vec::new()],
        "each change exactly once, in order"
    );
}

#[test]
fn a_voice_that_is_fading_out_is_already_not_playing() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    engine
        .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, 1)
        .expect("play");
    engine.stop_tag(1).expect("stop");
    // No audio callback has run since, so the voice still holds its slot.
    assert_eq!(engine.playing_tags(), Ok(Vec::new()));
}

#[test]
fn stopping_one_tag_leaves_the_others() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    engine
        .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, 1)
        .expect("one");
    engine
        .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, 2)
        .expect("two");
    engine.stop_tag(1).expect("stop");
    assert_eq!(engine.playing_tags(), Ok(vec![2]));
    engine
        .stop_tag(99)
        .expect("stopping an unknown tag is harmless");
    assert_eq!(engine.playing_tags(), Ok(vec![2]));
}

#[test]
fn a_rebuilt_stream_reports_that_nothing_is_playing_any_more() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    let seen = recorder(&engine);
    engine
        .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, 4)
        .expect("play");
    wait_for("the start", || reported(&seen).last() == Some(&vec![4]));

    shared.emit(StreamEvent::Invalidated);
    wait_for("the rebuild", || engine.snapshot().opens == 2);
    wait_for("the empty set", || {
        reported(&seen).last() == Some(&Vec::new())
    });
}

#[test]
fn a_replaced_listener_stops_hearing_and_a_quiet_engine_stays_quiet() {
    let shared = FakeShared::new();
    let engine = engine(&shared);
    wait_running(&engine);
    let first = recorder(&engine);
    let second = recorder(&engine);
    engine
        .play_tagged(clip(0.1, 48_000, STEREO_48K), 1.0, 8)
        .expect("play");
    wait_for("the second to hear it", || {
        reported(&second).last() == Some(&vec![8])
    });
    assert_eq!(
        reported(&first),
        vec![Vec::<u64>::new()],
        "the first heard only its initial state"
    );

    // Nothing changes: no further reports, however long the engine's poll loop runs.
    let before = reported(&second).len();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(reported(&second).len(), before);
}

#[test]
fn a_sound_that_ends_by_itself_is_noticed_quickly_even_with_a_slow_device_poll() {
    let shared = FakeShared::new();
    let backend_shared = Arc::clone(&shared);
    let engine = AudioEngine::start(
        EngineConfig {
            poll_interval: Duration::from_secs(30),
            retry_delays: vec![Duration::from_millis(10)],
        },
        move || Box::new(FakeBackend(backend_shared)),
    )
    .expect("engine thread");
    wait_running(&engine);
    let seen = recorder(&engine);
    engine
        .play_tagged(clip(0.1, 480, STEREO_48K), 1.0, 6)
        .expect("play");
    wait_for("the start", || reported(&seen).last() == Some(&vec![6]));
    shared.pump(2_000);
    // The device poll is 30 s away; only the voice watch can notice this within the test's deadline.
    wait_for("the end", || reported(&seen).last() == Some(&Vec::new()));
}
