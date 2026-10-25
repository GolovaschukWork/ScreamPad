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
