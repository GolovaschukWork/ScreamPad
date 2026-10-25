//! The single place where IPC commands are registered. The TypeScript bindings are generated from this registry.

use tauri_specta::{Builder, collect_commands};

use crate::commands;

/// Generic over the runtime so tests can use Tauri's mock runtime; production uses `tauri::Wry`.
pub fn builder<R: tauri::Runtime>() -> Builder<R> {
    Builder::<R>::new().commands(collect_commands![
        commands::app::app_info,
        commands::library::library_stats
    ])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};
    use specta_typescript::Typescript;
    use tauri::WebviewWindowBuilder;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::webview::InvokeRequest;

    use super::builder;
    use crate::commands::error::AppError;
    use crate::infrastructure::sqlite::{Database, StorageError, migrations};
    use crate::state::StorageState;

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
        let mut builder_app =
            mock_builder().invoke_handler(builder::<MockRuntime>().invoke_handler());
        if let Some(storage) = storage {
            builder_app = builder_app.manage(storage);
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
            Some(StorageState::Ready(Arc::new(database))),
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
            Some(StorageState::Ready(Arc::new(database))),
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
            StorageError::Io(
                "/Users/alice/Library/Application Support/secret/library.db".to_owned(),
            ),
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
}
