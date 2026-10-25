use serde::Serialize;
use specta::Type;
use tauri::State;

use super::error::AppError;
use super::library::{PlayModeDto, SoundSummary, summarize};
use crate::domain::settings::Settings;
use crate::state::{PlaybackState, StorageState};

/// Application settings as the UI sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    /// `system` or a supported language tag such as `en` or `uk`.
    pub language: String,
    /// Master volume in millibels (1/100 dB), -6000..=0.
    pub master_gain_millibels: i32,
}

impl From<Settings> for SettingsDto {
    fn from(settings: Settings) -> Self {
        Self {
            language: settings.language.as_str().to_owned(),
            master_gain_millibels: settings.master_gain.millibels(),
        }
    }
}

/// Reads the stored settings. A damaged settings row is reported as `storageCorrupt`, never replaced by defaults.
#[tauri::command]
#[specta::specta]
pub async fn get_settings(storage: State<'_, StorageState>) -> Result<SettingsDto, AppError> {
    let library = storage.library()?;
    tauri::async_runtime::spawn_blocking(move || library.preferences.settings())
        .await
        .map_err(|_| AppError::Internal)?
        .map(SettingsDto::from)
        .map_err(AppError::from)
}

/// Stores the language and the master volume, and applies the volume to the audio output.
#[tauri::command]
#[specta::specta]
pub async fn update_settings(
    storage: State<'_, StorageState>,
    playback: State<'_, PlaybackState>,
    language: String,
    master_gain_millibels: i32,
) -> Result<SettingsDto, AppError> {
    let library = storage.library()?;
    let service = playback.service().ok();
    tauri::async_runtime::spawn_blocking(move || -> Result<SettingsDto, AppError> {
        let settings = library
            .preferences
            .update_settings(&language, master_gain_millibels)?;
        if let Some(service) = service {
            // The value is stored either way; if the engine is busy it picks the stored value up at the next start.
            let _ = service.apply_master_gain(settings.master_gain);
        }
        Ok(settings.into())
    })
    .await
    .map_err(|_| AppError::Internal)?
}

/// Stores a sound's gain trim and play mode.
#[tauri::command]
#[specta::specta]
pub async fn set_sound_options(
    storage: State<'_, StorageState>,
    id: u32,
    gain_millibels: i32,
    play_mode: PlayModeDto,
) -> Result<SoundSummary, AppError> {
    let library = storage.library()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<SoundSummary, AppError> {
        let sound = library.preferences.set_sound_options(
            i64::from(id),
            gain_millibels,
            play_mode.into(),
        )?;
        summarize(&sound, library.status_of(sound.id))
    })
    .await
    .map_err(|_| AppError::Internal)?
}
