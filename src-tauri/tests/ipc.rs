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
use soundboard_desktop_lib::application::ports::FilePicker;
use soundboard_desktop_lib::commands::error::AppError;
use soundboard_desktop_lib::infrastructure::sqlite::{Database, migrations};
use soundboard_desktop_lib::ipc::builder;
use soundboard_desktop_lib::state::{PickerState, StorageState};

/// Regenerates `src/shared/ipc/bindings.ts`. CI fails when the committed file differs from the generated one.
#[test]
fn export_bindings() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/shared/ipc/bindings.ts");
    builder::<tauri::Wry>()
        .export(Typescript::default().header("// @ts-nocheck"), path)
        .expect("bindings export failed");
}

/// The webview's own origin. A remote origin is rejected by Tauri's ACL for custom commands, by design.
#[cfg(windows)]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// Sends a command through Tauri's IPC layer on the mock runtime, as the webview would.
fn invoke(cmd: &str, storage: Option<StorageState>) -> Result<Value, Value> {
    invoke_with(cmd, storage, None)
}

/// Same, with a file picker that answers without opening a dialog.
fn invoke_with(
    cmd: &str,
    storage: Option<StorageState>,
    picker: Option<PickerState>,
) -> Result<Value, Value> {
    let mut builder_app = mock_builder().invoke_handler(builder::<MockRuntime>().invoke_handler());
    if let Some(storage) = storage {
        builder_app = builder_app.manage(storage);
    }
    if let Some(picker) = picker {
        builder_app = builder_app.manage(picker);
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
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize().expect("response is JSON"))
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
    assert_eq!(empty, json!({ "soundCount": 0, "schemaVersion": 1 }));

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
    assert_eq!(one, json!({ "soundCount": 1, "schemaVersion": 1 }));
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
