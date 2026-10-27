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

/// Opens only a package explicitly chosen by the user. The standard macOS Installer handles
/// installation, confirmation and any administrator prompt; ScreamPad never elevates itself.
#[derive(Debug)]
pub struct InstallerOpenError;

#[cfg(target_os = "macos")]
pub fn open_audio_installer<R: Runtime>(app: &AppHandle<R>) -> Result<bool, InstallerOpenError> {
    let chosen = app
        .dialog()
        .file()
        .add_filter("ScreamPad Audio package", &["pkg"])
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok());
    let Some(path) = chosen else {
        return Ok(false);
    };
    let valid = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("ScreamPadAudio2ch-") && name.ends_with(".pkg"))
        && path.metadata().is_ok_and(|metadata| metadata.is_file());
    if !valid {
        return Err(InstallerOpenError);
    }
    std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg("Installer")
        .arg(path)
        .status()
        .map(|status| status.success())
        .map_err(|_| InstallerOpenError)
}

#[cfg(not(target_os = "macos"))]
pub fn open_audio_installer<R: Runtime>(_app: &AppHandle<R>) -> Result<bool, InstallerOpenError> {
    Err(InstallerOpenError)
}
