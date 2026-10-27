//! Explicit, session-only controls for macOS call routing.

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::error::AppError;
use crate::audio::backend::{RouteCounters, RouteDevices};
use crate::audio::engine::{FailureReason, RouteState};
use crate::state::{InstallerState, PlaybackState};

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RoutingStateDto {
    Stopped,
    Running,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RoutingFailureDto {
    NoDevice,
    PermissionDenied,
    Unsupported,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoutingCountersDto {
    pub input_rate: u32,
    pub output_rate: u32,
    pub queue_capacity: u32,
    pub input_buffer_frames: u32,
    pub output_buffer_frames: u32,
    pub captured: u32,
    pub rendered: u32,
    pub occupancy: u32,
    pub overflow: u32,
    pub underflow: u32,
    pub discarded: u32,
    pub max_occupancy: u32,
    pub clipped: u32,
    pub xruns: u32,
    pub clip_errors: u32,
}

impl From<RouteCounters> for RoutingCountersDto {
    fn from(value: RouteCounters) -> Self {
        let bounded = |number| u32::try_from(number).unwrap_or(u32::MAX);
        Self {
            input_rate: value.input_rate,
            output_rate: value.output_rate,
            queue_capacity: value.queue_capacity,
            input_buffer_frames: value.input_buffer_frames,
            output_buffer_frames: value.output_buffer_frames,
            captured: bounded(value.captured),
            rendered: bounded(value.rendered),
            occupancy: bounded(value.occupancy),
            overflow: bounded(value.overflow),
            underflow: bounded(value.underflow),
            discarded: bounded(value.discarded),
            max_occupancy: bounded(value.max_occupancy),
            clipped: bounded(value.clipped),
            xruns: bounded(value.xruns),
            clip_errors: bounded(value.clip_errors),
        }
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoutingStatusDto {
    pub state: RoutingStateDto,
    pub failure: Option<RoutingFailureDto>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub counters: Option<RoutingCountersDto>,
}

fn status(playback: &PlaybackState) -> Result<RoutingStatusDto, AppError> {
    let snapshot = playback.service()?.engine().route_snapshot();
    let (state, failure) = match snapshot.state {
        RouteState::Stopped => (RoutingStateDto::Stopped, None),
        RouteState::Running => (RoutingStateDto::Running, None),
        RouteState::Failed(reason) => (
            RoutingStateDto::Failed,
            Some(match reason {
                FailureReason::NoDevice => RoutingFailureDto::NoDevice,
                FailureReason::PermissionDenied => RoutingFailureDto::PermissionDenied,
                FailureReason::Unsupported => RoutingFailureDto::Unsupported,
                FailureReason::Other => RoutingFailureDto::Other,
            }),
        ),
    };
    Ok(RoutingStatusDto {
        state,
        failure,
        sample_rate: snapshot.format.map(|format| format.sample_rate),
        channels: snapshot.format.map(|format| format.channels),
        counters: snapshot.counters.map(Into::into),
    })
}

#[tauri::command]
#[specta::specta]
pub fn call_routing_status(
    playback: State<'_, PlaybackState>,
) -> Result<RoutingStatusDto, AppError> {
    status(&playback)
}

#[tauri::command]
#[specta::specta]
pub async fn start_call_routing(
    playback: State<'_, PlaybackState>,
    microphone_id: String,
    virtual_output_id: String,
) -> Result<RoutingStatusDto, AppError> {
    if microphone_id.is_empty()
        || virtual_output_id.is_empty()
        || microphone_id.len() > 512
        || virtual_output_id.len() > 512
    {
        return Err(AppError::InvalidSetting);
    }
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || {
        let engine = service.engine();
        engine
            .start_route(RouteDevices {
                microphone_id,
                virtual_output_id,
            })
            .map_err(|_| match engine.route_snapshot().state {
                RouteState::Failed(FailureReason::PermissionDenied) => {
                    AppError::RoutingPermissionDenied
                }
                RouteState::Failed(FailureReason::Unsupported) => AppError::RoutingUnsupported,
                RouteState::Failed(FailureReason::NoDevice) => AppError::RoutingDeviceUnavailable,
                _ => AppError::AudioBusy,
            })?;
        let snapshot = engine.route_snapshot();
        Ok(RoutingStatusDto {
            state: RoutingStateDto::Running,
            failure: None,
            sample_rate: snapshot.format.map(|format| format.sample_rate),
            channels: snapshot.format.map(|format| format.channels),
            counters: snapshot.counters.map(Into::into),
        })
    })
    .await
    .map_err(|_| AppError::Internal)?
}

#[tauri::command]
#[specta::specta]
pub async fn stop_call_routing(playback: State<'_, PlaybackState>) -> Result<(), AppError> {
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.engine().stop_route())
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(|_| AppError::AudioBusy)
}

#[tauri::command]
#[specta::specta]
pub async fn set_local_monitor_output(
    playback: State<'_, PlaybackState>,
    device_id: Option<String>,
) -> Result<(), AppError> {
    if device_id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 512)
    {
        return Err(AppError::InvalidSetting);
    }
    let service = playback.service()?;
    tauri::async_runtime::spawn_blocking(move || service.engine().set_local_output(device_id))
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(|_| AppError::RoutingUnsupported)
}

/// After an explicit click, let the user choose a locally built package and open it in macOS Installer.
/// The app never installs or elevates itself; the standard Installer owns confirmation and privileges.
#[tauri::command]
#[specta::specta]
pub async fn open_audio_installer(installer: State<'_, InstallerState>) -> Result<bool, AppError> {
    let open = installer.0.clone();
    tauri::async_runtime::spawn_blocking(move || open())
        .await
        .map_err(|_| AppError::Internal)?
        .map_err(|_| AppError::RoutingUnsupported)
}
