//! CPAL enumeration only. No stream is built, played, paused or changed here.

use std::collections::BTreeMap;

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, DeviceType, SampleFormat, SupportedStreamConfigRange};

use crate::application::device_discovery::{
    AudioDevice, ConfigurationStatus, DeviceDiscovery, DeviceKind, DeviceList, DiscoveryFailed,
};

pub struct CpalDeviceDiscovery;

impl DeviceDiscovery for CpalDeviceDiscovery {
    fn list(&self) -> Result<DeviceList, DiscoveryFailed> {
        // Construct the host on the blocking worker that owns this discovery call. The running
        // output engine owns its own backend and is never contacted by this read-only scan.
        let host = cpal::default_host();
        let mut devices = BTreeMap::<String, AudioDevice>::new();
        let mut incomplete = false;

        for device in host.input_devices().map_err(|_| DiscoveryFailed)? {
            record(&mut devices, entry(&device, &mut incomplete), true, false);
        }
        for device in host.output_devices().map_err(|_| DiscoveryFailed)? {
            let candidate = entry(&device, &mut incomplete).map(|mut entry| {
                entry.output = true;
                entry.configuration = output_configuration(&device, &mut incomplete);
                entry
            });
            record(&mut devices, candidate, false, true);
        }

        Ok(DeviceList {
            devices: devices.into_values().collect(),
            incomplete,
        })
    }
}

fn entry(device: &Device, incomplete: &mut bool) -> Option<AudioDevice> {
    let id = device.id().ok().map(|id| id.to_string());
    let (name, kind) = match device.description() {
        Ok(description) => (
            description.name().to_owned(),
            classify(description.device_type()),
        ),
        Err(_) => {
            *incomplete = true;
            (device.to_string(), DeviceKind::Unknown)
        }
    };
    entry_from_parts(id, name, kind, incomplete)
}

fn entry_from_parts(
    id: Option<String>,
    name: String,
    kind: DeviceKind,
    incomplete: &mut bool,
) -> Option<AudioDevice> {
    let Some(id) = id else {
        *incomplete = true;
        return None;
    };
    Some(AudioDevice {
        id,
        name,
        kind,
        input: false,
        output: false,
        configuration: ConfigurationStatus::Unknown,
    })
}

fn record(
    devices: &mut BTreeMap<String, AudioDevice>,
    candidate: Option<AudioDevice>,
    input: bool,
    output: bool,
) {
    let Some(entry) = candidate else { return };
    let found = devices
        .entry(entry.id.clone())
        .or_insert_with(|| entry.clone());
    found.input |= input;
    found.output |= output;
    if output {
        found.configuration = entry.configuration;
        found.kind = entry.kind;
        found.name = entry.name;
    }
}

fn classify(kind: DeviceType) -> DeviceKind {
    match kind {
        DeviceType::Virtual => DeviceKind::Virtual,
        DeviceType::Microphone
        | DeviceType::Speaker
        | DeviceType::Headphones
        | DeviceType::Headset
        | DeviceType::Earpiece
        | DeviceType::Handset
        | DeviceType::HearingAid
        | DeviceType::Dock => DeviceKind::Physical,
        _ => DeviceKind::Unknown,
    }
}

fn output_configuration(device: &Device, incomplete: &mut bool) -> ConfigurationStatus {
    match device.supported_output_configs() {
        Ok(mut ranges) => {
            if ranges.any(|range| compatible(&range)) {
                ConfigurationStatus::Supported
            } else {
                ConfigurationStatus::Unsupported
            }
        }
        Err(_) => {
            *incomplete = true;
            ConfigurationStatus::Unknown
        }
    }
}

fn compatible(range: &SupportedStreamConfigRange) -> bool {
    compatible_values(
        range.sample_format(),
        range.channels(),
        range.min_sample_rate(),
        range.max_sample_rate(),
    )
}

fn compatible_values(format: SampleFormat, channels: u16, min_rate: u32, max_rate: u32) -> bool {
    matches!(
        format,
        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
    ) && (1..=2).contains(&channels)
        && min_rate <= 192_000
        && max_rate >= 8_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_never_uses_a_provider_name() {
        assert_eq!(classify(DeviceType::Virtual), DeviceKind::Virtual);
        assert_eq!(classify(DeviceType::Speaker), DeviceKind::Physical);
        assert_eq!(classify(DeviceType::Microphone), DeviceKind::Physical);
        assert_eq!(classify(DeviceType::Unknown), DeviceKind::Unknown);
    }

    #[test]
    fn roles_join_by_id_and_names_are_not_identities() {
        let mut devices = BTreeMap::new();
        let mut incomplete = false;
        let input = entry_from_parts(
            Some("host:a".into()),
            "Same name".into(),
            DeviceKind::Unknown,
            &mut incomplete,
        );
        let mut output = entry_from_parts(
            Some("host:a".into()),
            "Same name".into(),
            DeviceKind::Virtual,
            &mut incomplete,
        );
        output.as_mut().expect("entry").configuration = ConfigurationStatus::Supported;
        record(&mut devices, input, true, false);
        record(&mut devices, output, false, true);
        record(
            &mut devices,
            entry_from_parts(
                Some("host:b".into()),
                "Same name".into(),
                DeviceKind::Unknown,
                &mut incomplete,
            ),
            false,
            true,
        );
        assert_eq!(devices.len(), 2);
        assert!(devices["host:a"].input && devices["host:a"].output);
        assert_eq!(devices["host:a"].kind, DeviceKind::Virtual);
        assert!(!devices["host:b"].input);
        assert!(!incomplete);
    }

    #[test]
    fn a_device_without_an_id_is_not_selected_by_its_name() {
        let mut incomplete = false;
        assert!(
            entry_from_parts(
                None,
                "A device".into(),
                DeviceKind::Virtual,
                &mut incomplete
            )
            .is_none()
        );
        assert!(incomplete);
    }

    #[test]
    fn unsupported_formats_channels_and_rates_are_not_candidates() {
        assert!(compatible_values(SampleFormat::F32, 2, 44_100, 96_000));
        assert!(!compatible_values(SampleFormat::F64, 2, 44_100, 96_000));
        assert!(!compatible_values(SampleFormat::F32, 0, 44_100, 96_000));
        assert!(!compatible_values(SampleFormat::F32, 16, 44_100, 96_000));
        assert!(!compatible_values(SampleFormat::F32, 33, 44_100, 96_000));
        assert!(!compatible_values(SampleFormat::F32, 2, 1_000, 7_999));
    }
}
