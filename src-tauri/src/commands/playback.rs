use serde::Serialize;
use specta::Type;
use tauri::State;

use super::error::AppError;
use crate::audio::engine::{EngineState, FailureReason};
use crate::state::{MediaStatus, PlaybackState, StorageState};

/// Where the audio output stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AudioStatusKind {
    Starting,
    Running,
    Recovering,
    Failed,
    Stopped,
}

/// Why the audio output failed, when it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AudioFailure {
    NoDevice,
    PermissionDenied,
    UnsupportedFormat,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub state: AudioStatusKind,
    pub failure: Option<AudioFailure>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
}

/// Plays a library sound. The first play of a sound decodes it, so the work runs on the blocking pool.
#[tauri::command]
#[specta::specta]
pub async fn play_sound(
    storage: State<'_, StorageState>,
    playback: State<'_, PlaybackState>,
    id: u32,
) -> Result<(), AppError> {
    let library = storage.library()?;
    // The last reconciliation already knows about a missing or damaged file; answer without touching the disk.
    match library.status_of(i64::from(id)) {
        MediaStatus::Available => {}
        MediaStatus::Missing => return Err(AppError::MediaMissing),
        MediaStatus::Damaged => return Err(AppError::MediaCorrupt),
    }
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.play(i64::from(id), 1.0))
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(AppError::from)
}

/// Stops every sound that is playing.
#[tauri::command]
#[specta::specta]
pub async fn stop_all_sounds(playback: State<'_, PlaybackState>) -> Result<(), AppError> {
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.stop_all())
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(AppError::from)
}

/// The state of the audio output, for the UI to show.
#[tauri::command]
#[specta::specta]
pub fn audio_status(playback: State<'_, PlaybackState>) -> Result<AudioStatus, AppError> {
    let snapshot = playback.service()?.engine().snapshot();
    let (state, failure) = match snapshot.state {
        EngineState::Starting => (AudioStatusKind::Starting, None),
        EngineState::Running => (AudioStatusKind::Running, None),
        EngineState::Recovering { .. } => (AudioStatusKind::Recovering, None),
        EngineState::Stopped => (AudioStatusKind::Stopped, None),
        EngineState::Failed(reason) => (
            AudioStatusKind::Failed,
            Some(match reason {
                FailureReason::NoDevice => AudioFailure::NoDevice,
                FailureReason::PermissionDenied => AudioFailure::PermissionDenied,
                FailureReason::Unsupported => AudioFailure::UnsupportedFormat,
                FailureReason::Other => AudioFailure::Other,
            }),
        ),
    };
    Ok(AudioStatus {
        state,
        failure,
        sample_rate: snapshot.format.map(|format| format.sample_rate),
        channels: snapshot.format.map(|format| format.channels),
    })
}

/// Asks a failed audio output to try again now.
#[tauri::command]
#[specta::specta]
pub fn retry_audio(playback: State<'_, PlaybackState>) -> Result<(), AppError> {
    playback
        .service()?
        .engine()
        .retry()
        .map_err(|_| AppError::AudioBusy)
}
