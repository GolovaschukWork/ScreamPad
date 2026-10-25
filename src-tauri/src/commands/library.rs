use serde::Serialize;
use specta::Type;
use tauri::State;

use super::error::AppError;
use crate::application::cancel::CancelToken;
use crate::application::error::StorageError;
use crate::application::import::ImportOutcome;
use crate::application::ports::SoundRepository;
use crate::domain::sound::Sound;
use crate::infrastructure::sqlite::Database;
use crate::state::{MediaStatus, PickerState, StorageState};

/// Summary of the sound library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub sound_count: u32,
    pub schema_version: u32,
}

/// Reads library statistics. The query runs on the blocking pool, never on the async runtime or an audio thread.
#[tauri::command]
#[specta::specta]
pub async fn library_stats(state: State<'_, StorageState>) -> Result<LibraryStats, AppError> {
    let database = state.database()?;
    tauri::async_runtime::spawn_blocking(move || read_stats(&database))
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(AppError::from)
}

fn read_stats(database: &Database) -> Result<LibraryStats, StorageError> {
    let reader = database.reader()?;
    let count: i64 = reader.query_row("SELECT count(*) FROM sounds", [], |row| row.get(0))?;
    let version: i64 = reader.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(LibraryStats {
        sound_count: u32::try_from(count).unwrap_or(u32::MAX),
        schema_version: u32::try_from(version).unwrap_or(u32::MAX),
    })
}

/// Whether a sound's managed file can be used, as of the last reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SoundStatus {
    Available,
    Missing,
    Damaged,
}

impl From<MediaStatus> for SoundStatus {
    fn from(status: MediaStatus) -> Self {
        match status {
            MediaStatus::Available => Self::Available,
            MediaStatus::Missing => Self::Missing,
            MediaStatus::Damaged => Self::Damaged,
        }
    }
}

/// A sound as shown in the library list.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundSummary {
    pub id: u32,
    pub display_name: String,
    /// Lowercase extension of the managed file, for example `wav`.
    pub format: String,
    pub duration_ms: u32,
    pub sample_rate: u32,
    pub channels: u16,
    /// Exact up to 2^53 bytes; larger values are rejected as an internal error.
    #[specta(type = f64)]
    pub size_bytes: u64,
    pub status: SoundStatus,
}

/// Largest integer a JSON number (an IEEE double) holds exactly.
const MAX_EXACT_NUMBER: u64 = 1 << 53;

fn summarize(sound: &Sound, status: MediaStatus) -> Result<SoundSummary, AppError> {
    if sound.size_bytes > MAX_EXACT_NUMBER {
        return Err(AppError::Internal);
    }
    Ok(SoundSummary {
        id: u32::try_from(sound.id).map_err(|_| AppError::Internal)?,
        display_name: sound.display_name.clone(),
        format: sound.format.extension().to_owned(),
        duration_ms: u32::try_from(sound.duration_ms).map_err(|_| AppError::Internal)?,
        sample_rate: sound.sample_rate,
        channels: sound.channels,
        size_bytes: sound.size_bytes,
        status: status.into(),
    })
}

/// Lists the library. A sound whose file is missing or damaged is still listed, with its status.
#[tauri::command]
#[specta::specta]
pub async fn list_sounds(state: State<'_, StorageState>) -> Result<Vec<SoundSummary>, AppError> {
    let library = state.library()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<SoundSummary>, AppError> {
        let sounds = library.repository.list()?;
        sounds
            .iter()
            .map(|sound| summarize(sound, library.status_of(sound.id)))
            .collect()
    })
    .await
    .map_err(|_| AppError::Internal)?
}

/// Checks that a sound's managed file is usable: `mediaMissing` or `mediaCorrupt` otherwise. The database row is never changed.
#[tauri::command]
#[specta::specta]
pub async fn check_sound_media(state: State<'_, StorageState>, id: u32) -> Result<(), AppError> {
    let library = state.library()?;
    match library.status_of(i64::from(id)) {
        MediaStatus::Available => Ok(()),
        MediaStatus::Missing => Err(AppError::MediaMissing),
        MediaStatus::Damaged => Err(AppError::MediaCorrupt),
    }
}

/// Result of a completed import.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub sound: SoundSummary,
    /// The same bytes were already in the library, so nothing was added.
    pub already_in_library: bool,
}

/// Asks the user for an audio file with a native dialog opened by the backend, then imports it. `None` means the user
/// cancelled the dialog. The webview never supplies a path, so it cannot make the app read an arbitrary file.
#[tauri::command]
#[specta::specta]
pub async fn import_sound(
    storage: State<'_, StorageState>,
    picker: State<'_, PickerState>,
) -> Result<Option<ImportResult>, AppError> {
    let library = storage.library()?;
    let picker = std::sync::Arc::clone(&picker.0);
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<ImportResult>, AppError> {
        let Some(path) = picker.pick_audio_file() else {
            return Ok(None);
        };
        let (sound, already_in_library) =
            match library.importer.import(&path, &CancelToken::new())? {
                ImportOutcome::Imported(sound) => (sound, false),
                ImportOutcome::AlreadyInLibrary(sound) => (sound, true),
            };
        let summary = summarize(&sound, library.status_of(sound.id))?;
        Ok(Some(ImportResult {
            sound: summary,
            already_in_library,
        }))
    })
    .await
    .map_err(|_| AppError::Internal)?
}
