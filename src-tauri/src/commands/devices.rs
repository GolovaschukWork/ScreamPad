//! Read-only device discovery exposed to the webview. Names are display-only; IDs are identities.

use serde::Serialize;
use specta::Type;

use super::error::AppError;
use crate::application::device_discovery::{
    AudioDevice, ConfigurationStatus, DeviceDiscoveryService, DeviceKind, DeviceList,
};
use crate::infrastructure::audio::device_discovery::CpalDeviceDiscovery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKindDto {
    Virtual,
    Physical,
    Unknown,
}

impl From<DeviceKind> for DeviceKindDto {
    fn from(kind: DeviceKind) -> Self {
        match kind {
            DeviceKind::Virtual => Self::Virtual,
            DeviceKind::Physical => Self::Physical,
            DeviceKind::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DeviceConfigurationDto {
    Supported,
    Unsupported,
    Unknown,
}

impl From<ConfigurationStatus> for DeviceConfigurationDto {
    fn from(status: ConfigurationStatus) -> Self {
        match status {
            ConfigurationStatus::Supported => Self::Supported,
            ConfigurationStatus::Unsupported => Self::Unsupported,
            ConfigurationStatus::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceDto {
    pub id: String,
    pub name: String,
    pub kind: DeviceKindDto,
    pub input: bool,
    pub output: bool,
    pub configuration: DeviceConfigurationDto,
}

impl From<AudioDevice> for AudioDeviceDto {
    fn from(device: AudioDevice) -> Self {
        Self {
            id: device.id,
            name: device.name,
            kind: device.kind.into(),
            input: device.input,
            output: device.output,
            configuration: device.configuration.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevicesDto {
    pub devices: Vec<AudioDeviceDto>,
    pub incomplete: bool,
}

impl From<DeviceList> for AudioDevicesDto {
    fn from(list: DeviceList) -> Self {
        Self {
            devices: list.devices.into_iter().map(AudioDeviceDto::from).collect(),
            incomplete: list.incomplete,
        }
    }
}

/// Lists current audio devices. This never builds a stream or captures microphone audio.
#[tauri::command]
#[specta::specta]
pub async fn list_audio_devices() -> Result<AudioDevicesDto, AppError> {
    tauri::async_runtime::spawn_blocking(|| DeviceDiscoveryService::new(CpalDeviceDiscovery).list())
        .await
        .map_err(|_| AppError::DeviceDiscoveryFailed)?
        .map(AudioDevicesDto::from)
        .map_err(|_| AppError::DeviceDiscoveryFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_dto_preserves_ids_roles_and_incomplete_state() {
        let dto = AudioDevicesDto::from(DeviceList {
            devices: vec![AudioDevice {
                id: "coreaudio:device-1".into(),
                name: "Display only".into(),
                kind: DeviceKind::Unknown,
                input: true,
                output: true,
                configuration: ConfigurationStatus::Unsupported,
            }],
            incomplete: true,
        });
        assert_eq!(dto.devices[0].id, "coreaudio:device-1");
        assert!(dto.devices[0].input && dto.devices[0].output);
        assert_eq!(dto.devices[0].kind, DeviceKindDto::Unknown);
        assert_eq!(
            dto.devices[0].configuration,
            DeviceConfigurationDto::Unsupported
        );
        assert!(dto.incomplete);
    }
}
