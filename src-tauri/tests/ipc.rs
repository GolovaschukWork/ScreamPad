//! IPC integration tests. They run the Tauri mock runtime through the command layer, as the webview would.
//!
//! They live in an integration test (not in the library's unit tests) on purpose: on Windows the test executable links the
//! webview runtime, which needs the Common Controls v6 manifest, and `build.rs` can attach it to integration tests only.

use std::sync::Arc;

use serde_json::{Value, json};
use specta_typescript::Typescript;
use tauri::WebviewWindowBuilder;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

use soundboard_desktop_lib::application::cancel::CancelToken;
use soundboard_desktop_lib::application::error::StorageError;
use soundboard_desktop_lib::application::import::{ImportError, ImportOutcome};
use soundboard_desktop_lib::application::playback::{
    PlaybackError, PlaybackLimits, PlaybackService,
};
use soundboard_desktop_lib::application::ports::FilePicker;
use soundboard_desktop_lib::audio::backend::{
    AudioBackend, BackendError, EventSink, OutputFormat, RenderFn, RunningStream,
};
use soundboard_desktop_lib::audio::engine::{AudioEngine, EngineConfig, EngineState};
use soundboard_desktop_lib::commands::error::AppError;
use soundboard_desktop_lib::infrastructure::media::decode::SymphoniaClipDecoder;
use soundboard_desktop_lib::infrastructure::sqlite::{Database, migrations};
use soundboard_desktop_lib::ipc::builder;
use soundboard_desktop_lib::state::{PickerState, PlaybackState, StorageState};

/// Regenerates `src/shared/ipc/bindings.ts`. CI fails when the committed file differs from the generated one.
#[test]
fn export_bindings() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/shared/ipc/bindings.ts");
    builder::<tauri::Wry>()
        .export(Typescript::default().header("// @ts-nocheck"), path)
        .expect("bindings export failed");
    // Specta emits spaces after union separators before doc comments. Keep generated bindings
    // deterministic and free of trailing whitespace without editing the generated file by hand.
    let generated = std::fs::read_to_string(path).expect("read generated bindings");
    let normalized = generated
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(path, normalized).expect("normalize generated bindings");
}

/// The webview's own origin. A remote origin is rejected by Tauri's ACL for custom commands, by design.
#[cfg(windows)]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// The application state a test wants registered, like the real `setup` does.
#[derive(Default)]
struct Parts {
    storage: Option<StorageState>,
    picker: Option<PickerState>,
    playback: Option<PlaybackState>,
}

/// Sends a command with a JSON body through Tauri's IPC layer on the mock runtime, as the webview would.
fn call(cmd: &str, body: Value, parts: Parts) -> Result<Value, Value> {
    let mut builder_app = mock_builder().invoke_handler(builder::<MockRuntime>().invoke_handler());
    if let Some(storage) = parts.storage {
        builder_app = builder_app.manage(storage);
    }
    if let Some(picker) = parts.picker {
        builder_app = builder_app.manage(picker);
    }
    if let Some(playback) = parts.playback {
        builder_app = builder_app.manage(playback);
    }
    let app = builder_app
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");
    get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("valid url"),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize().expect("response is JSON"))
}

/// Sends a command without arguments.
fn invoke(cmd: &str, storage: Option<StorageState>) -> Result<Value, Value> {
    call(
        cmd,
        json!({}),
        Parts {
            storage,
            ..Parts::default()
        },
    )
}

/// Same, with a file picker that answers without opening a dialog.
fn invoke_with(
    cmd: &str,
    storage: Option<StorageState>,
    picker: Option<PickerState>,
) -> Result<Value, Value> {
    call(
        cmd,
        json!({}),
        Parts {
            storage,
            picker,
            ..Parts::default()
        },
    )
}

/// A picker that returns a fixed answer, like a user who always chooses (or cancels) the same thing.
struct FixedPicker(Option<std::path::PathBuf>);

impl FilePicker for FixedPicker {
    fn pick_audio_file(&self) -> Option<std::path::PathBuf> {
        self.0.clone()
    }
}

fn picker(path: Option<std::path::PathBuf>) -> Option<PickerState> {
    Some(PickerState(Arc::new(FixedPicker(path))))
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("screampad-ipc-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");
    dir
}

#[test]
fn app_info_round_trip() {
    let json = invoke("app_info", None).expect("command failed");
    assert_eq!(json["name"], "ScreamPad");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn unknown_command_is_rejected() {
    assert!(invoke("does_not_exist", None).is_err());
}

#[test]
fn library_stats_reports_the_schema_and_the_row_count() {
    let dir = temp_dir("stats");
    let database = Database::open(&dir.join("library.db"), &migrations()).expect("open");
    let empty = invoke(
        "library_stats",
        Some(StorageState::ready(
            Arc::new(database),
            &dir.join("library"),
        )),
    )
    .expect("command failed");
    assert_eq!(empty, json!({ "soundCount": 0, "schemaVersion": 3 }));

    let database = Database::open(&dir.join("library.db"), &migrations()).expect("reopen");
    database
        .with_writer(|c| {
            c.execute(
                "INSERT INTO sounds (content_hash, display_name, managed_path, format, duration_ms, sample_rate, channels, \
                 size_bytes, created_at_ms, updated_at_ms) VALUES (?1, 'n', 'a/b.wav', 'wav', 1, 48000, 2, 1, 0, 0)",
                ["e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
            )?;
            Ok(())
        })
        .expect("insert");
    let one = invoke(
        "library_stats",
        Some(StorageState::ready(
            Arc::new(database),
            &dir.join("library"),
        )),
    )
    .expect("command failed");
    assert_eq!(one, json!({ "soundCount": 1, "schemaVersion": 3 }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_storage_failure_reaches_the_ui_as_a_typed_error() {
    let error = invoke(
        "library_stats",
        Some(StorageState::Unavailable(AppError::StorageCorrupt)),
    )
    .expect_err("must fail");
    assert_eq!(error, json!({ "kind": "storageCorrupt" }));
}

#[test]
fn a_damaged_database_is_reported_and_left_untouched() {
    let dir = temp_dir("corrupt");
    let path = dir.join("library.db");
    let garbage = vec![0xAB_u8; 4096];
    std::fs::write(&path, &garbage).expect("write garbage");

    let state = StorageState::open(&dir);
    assert!(matches!(
        state,
        StorageState::Unavailable(AppError::StorageCorrupt)
    ));
    assert_eq!(
        std::fs::read(&path).expect("still there"),
        garbage,
        "never modified automatically"
    );
    let error = invoke("library_stats", Some(state)).expect_err("must fail");
    assert_eq!(error, json!({ "kind": "storageCorrupt" }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_newer_database_is_reported_with_versions_only() {
    let error = AppError::from(StorageError::TooNew {
        database: 9,
        supported: 1,
    });
    assert_eq!(
        serde_json::to_value(error).expect("json"),
        json!({ "kind": "storageTooNew", "database": 9, "supported": 1 })
    );
}

#[test]
fn error_details_and_absolute_paths_never_reach_the_ui() {
    let secrets = [
        StorageError::Io("/Users/alice/Library/Application Support/secret/library.db".to_owned()),
        StorageError::Sql("C:\\Users\\bob\\secret.db: boom".to_owned()),
        StorageError::Migration("failed at /home/carol/x.sql".to_owned()),
    ];
    for secret in secrets {
        let json = serde_json::to_string(&AppError::from(&secret)).expect("json");
        for leak in [
            "/Users", "alice", "C:", "bob", "/home", "carol", "secret", "boom",
        ] {
            assert!(!json.contains(leak), "{json} leaks {leak}");
        }
        assert!(json.starts_with("{\"kind\":"), "{json}");
    }
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "{}/tests/fixtures/audio/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn library_with_two_sounds(name: &str) -> (std::path::PathBuf, StorageState) {
    let dir = temp_dir(name);
    let database = Database::open(&dir.join("library.db"), &migrations()).expect("open");
    let state = StorageState::ready(Arc::new(database), &dir.join("library"));
    let library = state.library().expect("ready");
    for fixture_name in ["tone.wav", "tone.flac"] {
        let outcome = library
            .importer
            .import(&fixture(fixture_name), &CancelToken::new());
        assert!(
            matches!(outcome, Ok(ImportOutcome::Imported(_))),
            "{outcome:?}"
        );
    }
    (dir, state)
}

#[test]
fn list_sounds_reports_imported_sounds_with_their_status() {
    let (dir, state) = library_with_two_sounds("list");
    let json = invoke("list_sounds", Some(state)).expect("command failed");
    let sounds = json.as_array().expect("an array");
    assert_eq!(sounds.len(), 2);
    assert_eq!(sounds[0]["displayName"], "tone");
    assert_eq!(sounds[0]["format"], "wav");
    assert_eq!(sounds[0]["sampleRate"], 8000);
    assert_eq!(sounds[0]["durationMs"], 250);
    assert_eq!(sounds[0]["status"], "available");
    assert_eq!(sounds[0]["gainMillibels"], 0);
    assert_eq!(sounds[0]["playMode"], "overlap");
    assert_eq!(sounds[1]["format"], "flac");
    assert!(sounds[0]["sizeBytes"].is_number());
    let text = json.to_string();
    for leak in ["/", "\\", "contentHash", "managedPath", "originalFileName"] {
        assert!(
            !text.contains(leak),
            "the list must not expose {leak}: {text}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_file_is_reported_as_media_missing_and_the_row_stays() {
    let (dir, state) = library_with_two_sounds("missing");
    let library = state.library().expect("ready");
    let sounds = soundboard_desktop_lib::application::ports::SoundRepository::list(
        library.repository.as_ref(),
    )
    .expect("rows");
    let victim = &sounds[0];
    let path = dir.join("library").join("media").join(victim.key.as_str());
    let mut permissions = std::fs::metadata(&path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).expect("permissions");
    std::fs::remove_file(&path).expect("remove the managed file");
    library.reconcile().expect("reconcile");

    let id = u32::try_from(victim.id).expect("small id");
    let check = |state: StorageState, id: u32| {
        let mut app = mock_builder()
            .invoke_handler(builder::<MockRuntime>().invoke_handler())
            .manage(state);
        app = app.manage(());
        let app = app.build(mock_context(noop_assets())).expect("app");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("webview");
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "check_sound_media".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: LOCAL_ORIGIN.parse().expect("valid url"),
                body: InvokeBody::Json(json!({ "id": id })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().expect("json"))
    };
    let library_state = StorageState::Ready(Arc::clone(&library));
    assert_eq!(
        check(library_state, id).expect_err("must be reported"),
        json!({ "kind": "mediaMissing" })
    );
    assert_eq!(
        check(
            StorageState::Ready(Arc::clone(&library)),
            u32::try_from(sounds[1].id).expect("id")
        ),
        Ok(Value::Null)
    );

    let listed = invoke("list_sounds", Some(StorageState::Ready(library))).expect("list");
    assert_eq!(
        listed.as_array().expect("array").len(),
        2,
        "the row is kept"
    );
    assert_eq!(listed[0]["status"], "missing");
    assert_eq!(listed[1]["status"], "available");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_import_failure_maps_to_a_typed_error_without_details() {
    let failures = [
        (ImportError::Busy, "importBusy"),
        (ImportError::Cancelled, "importCancelled"),
        (ImportError::SourceUnreadable, "importSourceUnreadable"),
        (
            ImportError::SourceNotRegularFile,
            "importSourceNotRegularFile",
        ),
        (ImportError::SourceChanged, "importSourceChanged"),
        (ImportError::Empty, "importEmpty"),
        (ImportError::TooLarge, "importTooLarge"),
        (
            ImportError::StagingQuotaExceeded,
            "importStagingQuotaExceeded",
        ),
        (ImportError::DiskFull, "importDiskFull"),
        (ImportError::UnsupportedFormat, "importUnsupportedFormat"),
        (ImportError::Corrupt, "importCorrupt"),
        (ImportError::TooLong, "importTooLong"),
        (ImportError::DecodeTimeout, "importDecodeTimeout"),
        (ImportError::LibraryInconsistent, "libraryInconsistent"),
        (
            ImportError::Io(std::io::ErrorKind::PermissionDenied),
            "storageIo",
        ),
        (ImportError::Storage(StorageError::Busy), "storageBusy"),
        (ImportError::Storage(StorageError::Full), "storageFull"),
    ];
    for (error, kind) in failures {
        assert_eq!(
            serde_json::to_value(AppError::from(&error)).expect("json"),
            json!({ "kind": kind }),
            "{error:?}"
        );
    }
}

fn empty_library(name: &str) -> (std::path::PathBuf, StorageState) {
    let dir = temp_dir(name);
    let database = Database::open(&dir.join("library.db"), &migrations()).expect("open");
    (
        dir.clone(),
        StorageState::ready(Arc::new(database), &dir.join("library")),
    )
}

#[test]
fn import_sound_imports_the_chosen_file_and_reports_a_repeat_as_already_in_the_library() {
    let (dir, state) = empty_library("import");
    let library = state.library().expect("ready");

    let first = invoke_with(
        "import_sound",
        Some(StorageState::Ready(Arc::clone(&library))),
        picker(Some(fixture("tone.wav"))),
    )
    .expect("import failed");
    assert_eq!(first["alreadyInLibrary"], false);
    assert_eq!(first["sound"]["displayName"], "tone");
    assert_eq!(first["sound"]["format"], "wav");
    assert_eq!(first["sound"]["status"], "available");

    let again = invoke_with(
        "import_sound",
        Some(StorageState::Ready(Arc::clone(&library))),
        picker(Some(fixture("tone.wav"))),
    )
    .expect("repeat failed");
    assert_eq!(again["alreadyInLibrary"], true);
    assert_eq!(again["sound"]["id"], first["sound"]["id"]);

    let listed = invoke("list_sounds", Some(StorageState::Ready(library))).expect("list");
    assert_eq!(listed.as_array().expect("array").len(), 1);
    let text = first.to_string();
    for leak in ["/", "\\", "fixtures", "contentHash", "managedPath"] {
        assert!(
            !text.contains(leak),
            "the result must not expose {leak}: {text}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancelling_the_dialog_imports_nothing() {
    let (dir, state) = empty_library("cancel");
    let library = state.library().expect("ready");
    let result = invoke_with(
        "import_sound",
        Some(StorageState::Ready(Arc::clone(&library))),
        picker(None),
    )
    .expect("command");
    assert_eq!(result, Value::Null);
    let listed = invoke("list_sounds", Some(StorageState::Ready(library))).expect("list");
    assert!(listed.as_array().expect("array").is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_unusable_file_is_reported_as_a_typed_error_and_nothing_is_added() {
    let (dir, state) = empty_library("unusable");
    let library = state.library().expect("ready");
    for (name, kind) in [
        ("garbage.bin", "importUnsupportedFormat"),
        ("not_audio.wav", "importUnsupportedFormat"),
        ("video_with_audio.mp4", "importUnsupportedFormat"),
    ] {
        let error = invoke_with(
            "import_sound",
            Some(StorageState::Ready(Arc::clone(&library))),
            picker(Some(fixture(name))),
        )
        .expect_err("must fail");
        assert_eq!(error, json!({ "kind": kind }), "{name}");
    }
    let missing = dir.join("does-not-exist.wav");
    let error = invoke_with(
        "import_sound",
        Some(StorageState::Ready(Arc::clone(&library))),
        picker(Some(missing)),
    )
    .expect_err("must fail");
    assert_eq!(error, json!({ "kind": "importSourceUnreadable" }));
    let listed = invoke("list_sounds", Some(StorageState::Ready(library))).expect("list");
    assert!(listed.as_array().expect("array").is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn import_sound_fails_cleanly_when_the_library_is_unavailable() {
    let error = invoke_with(
        "import_sound",
        Some(StorageState::Unavailable(AppError::StorageCorrupt)),
        picker(Some(fixture("tone.wav"))),
    )
    .expect_err("must fail");
    assert_eq!(error, json!({ "kind": "storageCorrupt" }));
}

/// The window must not be able to open the dialog itself or touch the file system through a plugin: only the backend picks files.
#[test]
fn the_window_capability_grants_no_dialog_or_file_system_permission() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities");
    for entry in std::fs::read_dir(path).expect("capabilities directory") {
        let file = entry.expect("entry").path();
        let text = std::fs::read_to_string(&file).expect("capability file");
        let json: Value = serde_json::from_str(&text).expect("valid JSON");
        let permissions = json["permissions"].as_array().expect("permissions array");
        for permission in permissions {
            let name = permission.as_str().unwrap_or_default();
            assert!(
                !name.starts_with("dialog:") && !name.starts_with("fs:"),
                "{file:?} grants {name}"
            );
        }
    }
}

#[test]
fn opener_capability_is_limited_to_two_publisher_urls() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities/default.json");
    let text = std::fs::read_to_string(path).expect("default capability");
    let capability: Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(
        capability["permissions"],
        json!([
            "core:default",
            {
                "identifier": "opener:allow-open-url",
                "allow": [
                    { "url": "https://existential.audio/blackhole/" },
                    { "url": "https://github.com/ExistentialAudio/BlackHole/wiki/Installation" }
                ]
            }
        ])
    );
}

// --- playback ---------------------------------------------------------------------------------------------------------

const TEST_FORMAT: OutputFormat = OutputFormat {
    sample_rate: 48_000,
    channels: 2,
};

type SharedRender = Arc<std::sync::Mutex<Option<RenderFn>>>;

/// A minimal audio API: it keeps the callback so a test can pump it by hand, or fails to open.
struct TestBackend {
    render: SharedRender,
    fail_with: Option<BackendError>,
}

struct TestStream;
impl RunningStream for TestStream {}

impl AudioBackend for TestBackend {
    fn open_default_output(
        &mut self,
        make_render: &mut dyn FnMut(OutputFormat) -> RenderFn,
        _events: EventSink,
    ) -> Result<(Box<dyn RunningStream>, OutputFormat), BackendError> {
        if let Some(error) = self.fail_with {
            return Err(error);
        }
        *self.render.lock().expect("render") = Some(make_render(TEST_FORMAT));
        Ok((Box::new(TestStream), TEST_FORMAT))
    }

    fn default_output_id(&mut self) -> Option<String> {
        Some("test-output".to_owned())
    }
}

fn playback_for(
    state: &StorageState,
    fail_with: Option<BackendError>,
) -> (PlaybackState, SharedRender) {
    let library = state.library().expect("ready");
    let render: SharedRender = Arc::new(std::sync::Mutex::new(None));
    let backend_render = Arc::clone(&render);
    let config = EngineConfig {
        poll_interval: std::time::Duration::from_millis(10),
        retry_delays: vec![std::time::Duration::from_millis(10)],
    };
    let engine = AudioEngine::start(config, move || {
        Box::new(TestBackend {
            render: backend_render,
            fail_with,
        })
    })
    .expect("engine");
    let service = PlaybackService::new(
        library.repository.clone(),
        library.files.clone(),
        Arc::new(SymphoniaClipDecoder),
        Arc::new(engine),
        PlaybackLimits::provisional(),
    );
    (PlaybackState::ready(service), render)
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for: {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn wait_running(playback: &PlaybackState) {
    let service = playback.service().expect("ready");
    wait_until("the engine to run", || {
        service.engine().snapshot().state == EngineState::Running
    });
}

fn pump(render: &SharedRender, frames: usize) -> Vec<f32> {
    let mut guard = render.lock().expect("render");
    let callback = guard.as_mut().expect("a stream is open");
    let mut out = vec![9.0; frames * 2];
    callback(&mut out);
    out
}

fn play(state: &StorageState, playback: PlaybackState, id: u32) -> Result<Value, Value> {
    call(
        "play_sound",
        json!({ "id": id }),
        Parts {
            storage: Some(StorageState::Ready(state.library().expect("ready"))),
            playback: Some(playback),
            ..Parts::default()
        },
    )
}

fn first_sound_id(state: &StorageState) -> u32 {
    let sounds = soundboard_desktop_lib::application::ports::SoundRepository::list(
        state.library().expect("ready").repository.as_ref(),
    )
    .expect("rows");
    u32::try_from(sounds[0].id).expect("small id")
}

#[test]
fn play_sound_makes_an_imported_sound_audible_and_stop_all_sounds_silences_it() {
    let (dir, storage) = library_with_two_sounds("play");
    let (playback, render) = playback_for(&storage, None);
    wait_running(&playback);
    let service = playback.service().expect("ready");
    let id = first_sound_id(&storage);

    let result =
        play(&storage, PlaybackState::Ready(Arc::clone(&service)), id).expect("play failed");
    assert_eq!(result, json!("started"));
    wait_until("sound", || {
        pump(&render, 256).iter().any(|s| s.abs() > 0.01)
    });

    let stopped = call(
        "stop_all_sounds",
        json!({}),
        Parts {
            playback: Some(PlaybackState::Ready(Arc::clone(&service))),
            ..Parts::default()
        },
    );
    assert_eq!(stopped, Ok(Value::Null));
    wait_until("silence", || pump(&render, 256).iter().all(|s| *s == 0.0));
    service.engine().shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn play_sound_reports_unknown_missing_and_damaged_sounds_as_typed_errors() {
    let (dir, storage) = library_with_two_sounds("play-errors");
    let (playback, _render) = playback_for(&storage, None);
    wait_running(&playback);
    let service = playback.service().expect("ready");

    let unknown = play(
        &storage,
        PlaybackState::Ready(Arc::clone(&service)),
        987_654,
    )
    .expect_err("must fail");
    assert_eq!(unknown, json!({ "kind": "soundNotFound" }));

    // A missing file found by the last reconciliation is answered without touching the disk or the decoder.
    let library = storage.library().expect("ready");
    let rows = soundboard_desktop_lib::application::ports::SoundRepository::list(
        library.repository.as_ref(),
    )
    .expect("rows");
    let path = dir.join("library").join("media").join(rows[0].key.as_str());
    let mut permissions = std::fs::metadata(&path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).expect("permissions");
    std::fs::remove_file(&path).expect("remove");
    library.reconcile().expect("reconcile");
    let missing = play(
        &storage,
        PlaybackState::Ready(Arc::clone(&service)),
        u32::try_from(rows[0].id).expect("id"),
    )
    .expect_err("must fail");
    assert_eq!(missing, json!({ "kind": "mediaMissing" }));

    // The other sound still plays.
    let fine = play(
        &storage,
        PlaybackState::Ready(Arc::clone(&service)),
        u32::try_from(rows[1].id).expect("id"),
    );
    assert_eq!(fine, Ok(json!("started")));
    service.engine().shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn audio_status_describes_a_running_output_and_a_failed_one() {
    let (dir, storage) = library_with_two_sounds("status");

    let (running, _render) = playback_for(&storage, None);
    wait_running(&running);
    let status = call(
        "audio_status",
        json!({}),
        Parts {
            playback: Some(PlaybackState::Ready(running.service().expect("ready"))),
            ..Parts::default()
        },
    )
    .expect("status");
    assert_eq!(
        status,
        json!({ "state": "running", "failure": null, "sampleRate": 48000, "channels": 2 })
    );
    running.shutdown();

    let (failing, _render) = playback_for(&storage, Some(BackendError::Unsupported));
    let service = failing.service().expect("ready");
    wait_until("failure", || {
        matches!(service.engine().snapshot().state, EngineState::Failed(_))
    });
    let status = call(
        "audio_status",
        json!({}),
        Parts {
            playback: Some(PlaybackState::Ready(Arc::clone(&service))),
            ..Parts::default()
        },
    )
    .expect("status");
    assert_eq!(
        status,
        json!({ "state": "failed", "failure": "unsupportedFormat", "sampleRate": null, "channels": null })
    );

    let refused = play(
        &storage,
        PlaybackState::Ready(Arc::clone(&service)),
        first_sound_id(&storage),
    )
    .expect_err("must fail");
    assert_eq!(refused, json!({ "kind": "audioUnavailable" }));
    let retried = call(
        "retry_audio",
        json!({}),
        Parts {
            playback: Some(PlaybackState::Ready(service)),
            ..Parts::default()
        },
    );
    assert_eq!(retried, Ok(Value::Null));
    failing.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn when_audio_is_unavailable_every_playback_command_says_so() {
    let (dir, storage) = library_with_two_sounds("unavailable");
    for (cmd, body) in [
        ("play_sound", json!({ "id": 1 })),
        ("stop_all_sounds", json!({})),
        ("stop_sound", json!({ "id": 1 })),
        ("playing_sounds", json!({})),
        ("audio_status", json!({})),
        ("retry_audio", json!({})),
    ] {
        let parts = Parts {
            storage: Some(StorageState::Ready(storage.library().expect("ready"))),
            playback: Some(PlaybackState::Unavailable(AppError::Internal)),
            ..Parts::default()
        };
        assert_eq!(
            call(cmd, body, parts),
            Err(json!({ "kind": "internal" })),
            "{cmd}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_playback_failure_maps_to_a_typed_error_without_details() {
    let failures = [
        (PlaybackError::SoundNotFound, "soundNotFound"),
        (PlaybackError::MediaMissing, "mediaMissing"),
        (PlaybackError::MediaCorrupt, "mediaCorrupt"),
        (PlaybackError::ClipTooLong, "playbackClipTooLong"),
        (PlaybackError::DecodeTimeout, "playbackDecodeTimeout"),
        (PlaybackError::AudioUnavailable, "audioUnavailable"),
        (PlaybackError::AudioBusy, "audioBusy"),
        (
            PlaybackError::Io(std::io::ErrorKind::PermissionDenied),
            "storageIo",
        ),
        (PlaybackError::Storage(StorageError::Full), "storageFull"),
    ];
    for (error, kind) in failures {
        assert_eq!(
            serde_json::to_value(AppError::from(&error)).expect("json"),
            json!({ "kind": kind }),
            "{error:?}"
        );
    }
}

// --- settings and sound options (ADR 0010) ----------------------------------------------------------------------------

/// Calls a command that needs the library; the audio side is registered as unavailable, as it is when the engine cannot start.
fn call_with_storage(cmd: &str, body: Value, state: &StorageState) -> Result<Value, Value> {
    call(
        cmd,
        body,
        Parts {
            storage: Some(StorageState::Ready(state.library().expect("ready"))),
            playback: Some(PlaybackState::Unavailable(AppError::Internal)),
            ..Parts::default()
        },
    )
}

#[test]
fn settings_default_then_persist_what_the_user_chose() {
    let (dir, state) = library_with_two_sounds("settings");
    assert_eq!(
        call_with_storage("get_settings", json!({}), &state),
        Ok(json!({ "language": "system", "masterGainMillibels": 0, "theme": "system" }))
    );
    let saved = call_with_storage(
        "update_settings",
        json!({ "language": "uk", "masterGainMillibels": -1500, "theme": "dark" }),
        &state,
    );
    assert_eq!(
        saved,
        Ok(json!({ "language": "uk", "masterGainMillibels": -1500, "theme": "dark" }))
    );
    assert_eq!(
        call_with_storage("get_settings", json!({}), &state),
        saved,
        "reads back what was stored"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_settings_are_rejected_without_changing_anything() {
    let (dir, state) = library_with_two_sounds("settings-invalid");
    for (language, gain) in [
        ("de", 0),
        ("", 0),
        ("UK", 0),
        ("uk", 1),
        ("uk", -6001),
        ("uk", i32::MIN),
        ("../x", 0),
    ] {
        assert_eq!(
            call_with_storage(
                "update_settings",
                json!({ "language": language, "masterGainMillibels": gain, "theme": "system" }),
                &state
            ),
            Err(json!({ "kind": "invalidSetting" })),
            "{language:?} {gain}"
        );
    }
    assert_eq!(
        call_with_storage("get_settings", json!({}), &state),
        Ok(json!({ "language": "system", "masterGainMillibels": 0, "theme": "system" }))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_settings_row_is_reported_not_hidden() {
    let (dir, state) = library_with_two_sounds("settings-missing");
    state
        .library()
        .expect("ready")
        .database
        .with_writer(|c| Ok(c.execute("DELETE FROM app_settings", [])?))
        .expect("delete");
    assert_eq!(
        call_with_storage("get_settings", json!({}), &state),
        Err(json!({ "kind": "storageCorrupt" }))
    );
    assert_eq!(
        call_with_storage(
            "update_settings",
            json!({ "language": "en", "masterGainMillibels": 0, "theme": "system" }),
            &state
        ),
        Err(json!({ "kind": "storageCorrupt" }))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn updating_settings_applies_the_master_volume_to_the_running_output() {
    let (dir, storage) = library_with_two_sounds("settings-master");
    let (playback, render) = playback_for(&storage, None);
    wait_running(&playback);
    let service = playback.service().expect("ready");
    let id = first_sound_id(&storage);

    let louder = |gain: i32| {
        let updated = call(
            "update_settings",
            json!({ "language": "system", "masterGainMillibels": gain, "theme": "system" }),
            Parts {
                storage: Some(StorageState::Ready(storage.library().expect("ready"))),
                playback: Some(PlaybackState::Ready(Arc::clone(&service))),
                ..Parts::default()
            },
        );
        assert!(updated.is_ok(), "{updated:?}");
        play(&storage, PlaybackState::Ready(Arc::clone(&service)), id).expect("play");
        pump(&render, 300);
        let peak = pump(&render, 256)
            .iter()
            .fold(0.0_f32, |p, s| p.max(s.abs()));
        call(
            "stop_all_sounds",
            json!({}),
            Parts {
                playback: Some(PlaybackState::Ready(Arc::clone(&service))),
                ..Parts::default()
            },
        )
        .expect("stop");
        wait_until("silence", || pump(&render, 512).iter().all(|s| *s == 0.0));
        peak
    };
    let unity = louder(0);
    let quiet = louder(-2000);
    assert!(unity > 0.05, "{unity}");
    assert!((quiet - unity * 0.1).abs() < 1e-3, "{quiet} vs {unity}");
    service.engine().shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn settings_can_be_saved_while_audio_is_unavailable() {
    let (dir, state) = library_with_two_sounds("settings-no-audio");
    let saved = call(
        "update_settings",
        json!({ "language": "en", "masterGainMillibels": -300, "theme": "light" }),
        Parts {
            storage: Some(StorageState::Ready(state.library().expect("ready"))),
            playback: Some(PlaybackState::Unavailable(AppError::Internal)),
            ..Parts::default()
        },
    );
    assert_eq!(
        saved,
        Ok(json!({ "language": "en", "masterGainMillibels": -300, "theme": "light" }))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sound_options_are_stored_validated_and_listed() {
    let (dir, state) = library_with_two_sounds("options");
    let id = first_sound_id(&state);
    let saved = call_with_storage(
        "set_sound_options",
        json!({ "id": id, "gainMillibels": -600, "playMode": "toggle" }),
        &state,
    )
    .expect("set");
    assert_eq!(saved["gainMillibels"], -600);
    assert_eq!(saved["playMode"], "toggle");
    assert_eq!(saved["displayName"], "tone");

    let listed = invoke(
        "list_sounds",
        Some(StorageState::Ready(state.library().expect("ready"))),
    )
    .expect("list");
    assert_eq!(listed[0]["playMode"], "toggle");
    assert_eq!(listed[0]["gainMillibels"], -600);
    assert_eq!(
        listed[1]["playMode"], "overlap",
        "the other sound is untouched"
    );

    for gain in [601, -6001, i32::MAX] {
        assert_eq!(
            call_with_storage(
                "set_sound_options",
                json!({ "id": id, "gainMillibels": gain, "playMode": "overlap" }),
                &state
            ),
            Err(json!({ "kind": "invalidSetting" })),
            "{gain}"
        );
    }
    assert_eq!(
        call_with_storage(
            "set_sound_options",
            json!({ "id": 987_654, "gainMillibels": 0, "playMode": "overlap" }),
            &state
        ),
        Err(json!({ "kind": "soundNotFound" }))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_unknown_play_mode_never_reaches_the_service() {
    let (dir, state) = library_with_two_sounds("options-mode");
    let id = first_sound_id(&state);
    for mode in ["loop", "OVERLAP", "", "restart "] {
        let refused = call_with_storage(
            "set_sound_options",
            json!({ "id": id, "gainMillibels": 0, "playMode": mode }),
            &state,
        );
        assert!(
            refused.is_err(),
            "{mode:?} must not be accepted: {refused:?}"
        );
    }
    let listed = invoke(
        "list_sounds",
        Some(StorageState::Ready(state.library().expect("ready"))),
    )
    .expect("list");
    assert_eq!(listed[0]["playMode"], "overlap");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn play_sound_reports_what_the_play_mode_did() {
    let (dir, storage) = library_with_two_sounds("play-modes");
    let (playback, _render) = playback_for(&storage, None);
    wait_running(&playback);
    let service = playback.service().expect("ready");
    let id = first_sound_id(&storage);
    call_with_storage(
        "set_sound_options",
        json!({ "id": id, "gainMillibels": 0, "playMode": "toggle" }),
        &storage,
    )
    .expect("set");

    let trigger = || play(&storage, PlaybackState::Ready(Arc::clone(&service)), id);
    assert_eq!(trigger(), Ok(json!("started")));
    assert_eq!(trigger(), Ok(json!("stopped")));
    call_with_storage(
        "set_sound_options",
        json!({ "id": id, "gainMillibels": 0, "playMode": "ignore" }),
        &storage,
    )
    .expect("set");
    assert_eq!(trigger(), Ok(json!("started")));
    assert_eq!(trigger(), Ok(json!("ignored")));
    service.engine().shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn preference_failures_map_to_typed_errors() {
    use soundboard_desktop_lib::application::preferences::PreferencesError;
    for (error, kind) in [
        (PreferencesError::InvalidValue, "invalidSetting"),
        (PreferencesError::SoundNotFound, "soundNotFound"),
        (
            PreferencesError::Storage(StorageError::Corrupt),
            "storageCorrupt",
        ),
    ] {
        assert_eq!(
            serde_json::to_value(AppError::from(&error)).expect("json"),
            json!({ "kind": kind }),
            "{error:?}"
        );
    }
}

#[test]
fn an_unknown_theme_never_reaches_the_service_and_changes_nothing() {
    let (dir, state) = library_with_two_sounds("theme-invalid");
    for theme in ["blue", "Dark", "", "auto"] {
        let refused = call_with_storage(
            "update_settings",
            json!({ "language": "en", "masterGainMillibels": 0, "theme": theme }),
            &state,
        );
        assert!(
            refused.is_err(),
            "{theme:?} must not be accepted: {refused:?}"
        );
    }
    assert_eq!(
        call_with_storage("get_settings", json!({}), &state),
        Ok(json!({ "language": "system", "masterGainMillibels": 0, "theme": "system" }))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_theme_is_stored_with_the_other_settings() {
    let (dir, state) = library_with_two_sounds("theme-stored");
    for theme in ["dark", "light", "system"] {
        let saved = call_with_storage(
            "update_settings",
            json!({ "language": "uk", "masterGainMillibels": -600, "theme": theme }),
            &state,
        )
        .expect("save");
        assert_eq!(saved["theme"], theme);
        assert_eq!(
            call_with_storage("get_settings", json!({}), &state).expect("get")["theme"],
            theme
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

// --- stopping one sound and the playing state ---------------------------------------------------------------------------

fn playback_call(cmd: &str, body: Value, service: &Arc<PlaybackService>) -> Result<Value, Value> {
    call(
        cmd,
        body,
        Parts {
            playback: Some(PlaybackState::Ready(Arc::clone(service))),
            ..Parts::default()
        },
    )
}

#[test]
fn playing_sounds_lists_what_sounds_and_stop_sound_removes_only_that_sound() {
    let (dir, storage) = library_with_two_sounds("playing");
    let (playback, render) = playback_for(&storage, None);
    wait_running(&playback);
    let service = playback.service().expect("ready");
    let sounds = soundboard_desktop_lib::application::ports::SoundRepository::list(
        storage.library().expect("ready").repository.as_ref(),
    )
    .expect("rows");
    let (first, second) = (
        u32::try_from(sounds[0].id).expect("id"),
        u32::try_from(sounds[1].id).expect("id"),
    );

    assert_eq!(
        playback_call("playing_sounds", json!({}), &service),
        Ok(json!([]))
    );
    for id in [first, second] {
        play(&storage, PlaybackState::Ready(Arc::clone(&service)), id).expect("play");
    }
    assert_eq!(
        playback_call("playing_sounds", json!({}), &service),
        Ok(json!([first, second]))
    );

    assert_eq!(
        playback_call("stop_sound", json!({ "id": first }), &service),
        Ok(Value::Null)
    );
    assert_eq!(
        playback_call("playing_sounds", json!({}), &service),
        Ok(json!([second]))
    );
    wait_until("the other sound to keep playing", || {
        pump(&render, 256).iter().any(|s| s.abs() > 0.01)
    });

    // Stopping a sound that is not playing, or unknown, is not an error.
    for id in [first, 987_654] {
        assert_eq!(
            playback_call("stop_sound", json!({ "id": id }), &service),
            Ok(Value::Null)
        );
    }
    service.engine().shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn playing_ids_keep_only_ids_the_ui_can_hold() {
    use soundboard_desktop_lib::commands::playback::playing_ids;
    assert_eq!(playing_ids(&[]), Vec::<u32>::new());
    assert_eq!(playing_ids(&[1, 7]), vec![1, 7]);
    assert_eq!(playing_ids(&[3, u64::from(u32::MAX) + 1, 4]), vec![3, 4]);
}

#[test]
fn playing_changes_reach_the_webview_as_a_typed_event() {
    use soundboard_desktop_lib::commands::playback::{PlayingChanged, forward_playing_changes};
    use tauri::Listener;

    let (dir, storage) = library_with_two_sounds("playing-event");
    let (playback, _render) = playback_for(&storage, None);
    wait_running(&playback);
    let service = playback.service().expect("ready");
    let id = first_sound_id(&storage);

    let ipc = builder::<MockRuntime>();
    let app = mock_builder()
        .invoke_handler(ipc.invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("mock app");
    ipc.mount_events(&app);
    let seen = Arc::new(std::sync::Mutex::new(Vec::<Vec<u32>>::new()));
    let sink = Arc::clone(&seen);
    app.listen(
        <PlayingChanged as tauri_specta::Event>::NAME,
        move |event| {
            let parsed: PlayingChanged = serde_json::from_str(event.payload()).expect("payload");
            sink.lock().expect("seen").push(parsed.ids);
        },
    );
    forward_playing_changes(&service, app.handle().clone());

    wait_until("the initial empty state", || {
        seen.lock().expect("seen").first() == Some(&Vec::new())
    });
    play(&storage, PlaybackState::Ready(Arc::clone(&service)), id).expect("play");
    wait_until("the start", || {
        seen.lock().expect("seen").last() == Some(&vec![id])
    });
    playback_call("stop_sound", json!({ "id": id }), &service).expect("stop");
    wait_until("the stop", || {
        seen.lock().expect("seen").last() == Some(&Vec::new())
    });
    service.engine().shutdown();
    let _ = std::fs::remove_dir_all(dir);
}
