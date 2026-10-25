pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod ipc;
pub mod state;

#[cfg(test)]
mod test_support;

use tauri::Manager;

use commands::error::AppError;
use state::StorageState;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(ipc::builder::<tauri::Wry>().invoke_handler())
        .setup(|app| {
            let storage = match app.path().app_data_dir() {
                Ok(dir) => StorageState::open(&dir),
                Err(_) => StorageState::Unavailable(AppError::StorageIo),
            };
            app.manage(storage);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
