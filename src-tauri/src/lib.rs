pub mod application;
pub mod audio;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod ipc;
pub mod state;

#[cfg(test)]
mod test_support;

use tauri::Manager;

use commands::error::AppError;
use infrastructure::dialog::TauriFilePicker;
use state::{PickerState, PlaybackState, StorageState};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(ipc::builder::<tauri::Wry>().invoke_handler())
        .setup(|app| {
            let storage = match app.path().app_data_dir() {
                Ok(dir) => StorageState::open(&dir),
                Err(_) => StorageState::Unavailable(AppError::StorageIo),
            };
            let library = storage.library().ok();
            let playback = PlaybackState::start(&storage);
            app.manage(storage);
            app.manage(playback);
            app.manage(PickerState(std::sync::Arc::new(TauriFilePicker::new(
                app.handle().clone(),
            ))));
            if let Some(library) = library {
                // Cleanup after interrupted imports runs in the background and never delays the window.
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = library.reconcile();
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Stop the audio stream and its thread before the process ends.
            if let tauri::RunEvent::Exit = event
                && let Some(playback) = app.try_state::<PlaybackState>()
            {
                playback.shutdown();
            }
        });
}
