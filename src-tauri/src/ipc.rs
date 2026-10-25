//! The single place where IPC commands are registered. The TypeScript bindings are generated from this registry.

use tauri_specta::{Builder, collect_commands};

use crate::commands;

/// Generic over the runtime so tests can use Tauri's mock runtime; production uses `tauri::Wry`.
pub fn builder<R: tauri::Runtime>() -> Builder<R> {
    Builder::<R>::new().commands(collect_commands![commands::app::app_info])
}

#[cfg(test)]
mod tests {
    use specta_typescript::Typescript;
    use tauri::WebviewWindowBuilder;
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::webview::InvokeRequest;

    use super::builder;

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
    fn invoke(cmd: &str) -> Result<InvokeResponseBody, serde_json::Value> {
        let app = mock_builder()
            .invoke_handler(builder::<MockRuntime>().invoke_handler())
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
    }

    #[test]
    fn app_info_round_trip() {
        let response = invoke("app_info").expect("command failed");
        let json: serde_json::Value = response.deserialize().expect("response is JSON");
        assert_eq!(json["name"], "ScreamPad");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn unknown_command_is_rejected() {
        assert!(invoke("does_not_exist").is_err());
    }
}
