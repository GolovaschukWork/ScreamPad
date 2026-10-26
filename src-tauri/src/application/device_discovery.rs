//! Read-only audio-device discovery, independent of the playback engine and Tauri.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Virtual,
    Physical,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationStatus {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// CPAL host-qualified identity. A display name must never be used as an identity.
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub input: bool,
    pub output: bool,
    pub configuration: ConfigurationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceList {
    pub devices: Vec<AudioDevice>,
    /// At least one device could not be identified or fully inspected.
    pub incomplete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryFailed;

/// Runs outside the audio callback. Implementations must only inspect devices, never open streams.
pub trait DeviceDiscovery: Send + Sync {
    fn list(&self) -> Result<DeviceList, DiscoveryFailed>;
}

pub struct DeviceDiscoveryService<D> {
    discovery: D,
}

impl<D: DeviceDiscovery> DeviceDiscoveryService<D> {
    pub fn new(discovery: D) -> Self {
        Self { discovery }
    }

    pub fn list(&self) -> Result<DeviceList, DiscoveryFailed> {
        self.discovery.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeDiscovery(Result<DeviceList, DiscoveryFailed>);

    impl DeviceDiscovery for FakeDiscovery {
        fn list(&self) -> Result<DeviceList, DiscoveryFailed> {
            self.0.clone()
        }
    }

    #[test]
    fn discovery_does_not_require_or_change_a_playback_engine() {
        let expected = DeviceList {
            devices: vec![AudioDevice {
                id: "coreaudio:loopback".into(),
                name: "Loopback".into(),
                kind: DeviceKind::Virtual,
                input: true,
                output: true,
                configuration: ConfigurationStatus::Supported,
            }],
            incomplete: false,
        };
        let service = DeviceDiscoveryService::new(FakeDiscovery(Ok(expected.clone())));
        assert_eq!(service.list(), Ok(expected));
    }

    #[test]
    fn a_discovery_failure_is_not_an_empty_device_list() {
        let service = DeviceDiscoveryService::new(FakeDiscovery(Err(DiscoveryFailed)));
        assert_eq!(service.list(), Err(DiscoveryFailed));
    }
}
