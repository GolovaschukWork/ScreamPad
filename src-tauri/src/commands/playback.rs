use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Runtime, State};
use tauri_specta::Event;

use super::error::AppError;
use crate::application::playback::PlayOutcome;
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

/// What triggering a sound did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PlayResult {
    Started,
    Restarted,
    Stopped,
    Ignored,
}

impl From<PlayOutcome> for PlayResult {
    fn from(outcome: PlayOutcome) -> Self {
        match outcome {
            PlayOutcome::Started => Self::Started,
            PlayOutcome::Restarted => Self::Restarted,
            PlayOutcome::Stopped => Self::Stopped,
            PlayOutcome::Ignored => Self::Ignored,
        }
    }
}

/// Triggers a library sound with its stored gain and play mode. The first play of a sound decodes it, so the work runs on
/// the blocking pool.
#[tauri::command]
#[specta::specta]
pub async fn play_sound(
    storage: State<'_, StorageState>,
    playback: State<'_, PlaybackState>,
    id: u32,
) -> Result<PlayResult, AppError> {
    let library = storage.library()?;
    // The last reconciliation already knows about a missing or damaged file; answer without touching the disk.
    match library.status_of(i64::from(id)) {
        MediaStatus::Available => {}
        MediaStatus::Missing => return Err(AppError::MediaMissing),
        MediaStatus::Damaged => return Err(AppError::MediaCorrupt),
    }
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.play(i64::from(id)))
        .await
        .map_err(|_| AppError::Internal)?
        .map(PlayResult::from)
        .map_err(AppError::from)
}

/// Which sounds are sounding right now. Sent whenever the set changes, and once when the listener is registered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct PlayingChanged {
    /// Ids of the sounds with a voice playing, ascending.
    pub ids: Vec<u32>,
}

/// Library ids as the UI knows them. An id that does not fit is dropped (the UI could not have listed that sound).
pub fn playing_ids(tags: &[u64]) -> Vec<u32> {
    tags.iter()
        .filter_map(|tag| u32::try_from(*tag).ok())
        .collect()
}

/// Forwards the engine's playing-set changes to the webview as `PlayingChanged` events.
pub fn forward_playing_changes<R: Runtime>(
    service: &crate::application::playback::PlaybackService,
    app: AppHandle<R>,
) {
    // A failed registration leaves the UI on `playing_sounds` polling; nothing else depends on the event.
    let _ = service.engine().on_playing_changed(move |tags| {
        let _ = PlayingChanged {
            ids: playing_ids(tags),
        }
        .emit(&app);
    });
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

/// Stops one sound (every voice of it). Stopping a sound that is not playing is not an error.
#[tauri::command]
#[specta::specta]
pub async fn stop_sound(playback: State<'_, PlaybackState>, id: u32) -> Result<(), AppError> {
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.stop(i64::from(id)))
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(AppError::from)
}

/// The sounds that are sounding right now. Use it for the initial state; changes arrive as `PlayingChanged` events.
#[tauri::command]
#[specta::specta]
pub async fn playing_sounds(playback: State<'_, PlaybackState>) -> Result<Vec<u32>, AppError> {
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.playing_sounds())
        .await
        .map_err(|_| AppError::Internal)?
        .map(|ids| {
            ids.into_iter()
                .filter_map(|id| u32::try_from(id).ok())
                .collect()
        })
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
