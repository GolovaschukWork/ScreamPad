use serde::Serialize;
use specta::Type;
use tauri::State;

use super::error::AppError;
use crate::infrastructure::sqlite::{Database, StorageError};
use crate::state::StorageState;

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
