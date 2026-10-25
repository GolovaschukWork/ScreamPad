//! The native file dialog, opened from Rust so that the webview never chooses a path.

use std::path::PathBuf;

use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

use crate::application::ports::FilePicker;
use crate::domain::sound::AudioFormat;

pub struct TauriFilePicker<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriFilePicker<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> FilePicker for TauriFilePicker<R> {
    fn pick_audio_file(&self) -> Option<PathBuf> {
        let extensions: Vec<&str> = AudioFormat::ALL
            .iter()
            .map(|format| format.extension())
            .collect();
        // The filter is a convenience only; the chosen file is validated by its content during the import.
        self.app
            .dialog()
            .file()
            .add_filter("Audio", &extensions)
            .blocking_pick_file()
            .and_then(|chosen| chosen.into_path().ok())
    }
}
