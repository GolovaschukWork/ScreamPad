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
    let ipc = ipc::builder::<tauri::Wry>();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(ipc.invoke_handler())
        .setup(move |app| {
            ipc.mount_events(app);
            let storage = match app.path().app_data_dir() {
                Ok(dir) => StorageState::open(&dir),
                Err(_) => StorageState::Unavailable(AppError::StorageIo),
            };
            let library = storage.library().ok();
            let playback = PlaybackState::start(&storage);
            if let Ok(service) = playback.service() {
                commands::playback::forward_playing_changes(&service, app.handle().clone());
            }
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
