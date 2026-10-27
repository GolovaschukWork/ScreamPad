import type { AudioDeviceDto, AudioDevicesDto } from "../../shared/ipc/bindings";

export type SelectedDevice = { id: string; name: string };
export type DiscoveryState =
  | { status: "scanning" }
  | { status: "failed" }
  | { status: "ready"; result: AudioDevicesDto };
export type RoutingState =
  | "scanning"
  | "setupRequired"
  | "candidateDetected"
  | "deviceSelected"
  | "deviceUnavailable"
  | "discoveryFailed"
  | "unsupportedConfiguration";

let selectedDevice: SelectedDevice | null = null;
let selectedMicrophone: SelectedDevice | null = null;
let selectedMonitor: SelectedDevice | null = null;

export function getSelectedDevice(): SelectedDevice | null {
  return selectedDevice;
}

export function setSelectedDevice(device: SelectedDevice | null): void {
  selectedDevice = device;
}

export function getSelectedMicrophone(): SelectedDevice | null { return selectedMicrophone; }
export function setSelectedMicrophone(device: SelectedDevice | null): void { selectedMicrophone = device; }
export function getSelectedMonitor(): SelectedDevice | null { return selectedMonitor; }
export function setSelectedMonitor(device: SelectedDevice | null): void { selectedMonitor = device; }

export function routingState(discovery: DiscoveryState, selected: SelectedDevice | null): RoutingState {
  if (discovery.status === "scanning") return "scanning";
  if (discovery.status === "failed") return "discoveryFailed";

  const devices = discovery.result.devices;
  if (selected) {
    const current = devices.find((device) => device.id === selected.id && device.output);
    if (!current) return "deviceUnavailable";
    return !current.input || current.configuration === "unsupported"
      ? "unsupportedConfiguration"
      : "deviceSelected";
  }

  const candidates = selectableDevices(devices).filter((device) => device.input);
  if (candidates.some((device) => device.configuration !== "unsupported")) return "candidateDetected";
  if (candidates.length > 0) return "unsupportedConfiguration";
  return "setupRequired";
}

export function selectableDevices(devices: AudioDeviceDto[]): AudioDeviceDto[] {
  return devices.filter((device) => device.output && device.kind !== "physical");
}

// A known, uniquely identified virtual route needs no extra choice in the common path.
// Never guess among third-party devices or from a display name.
const SCREAM_PAD_AUDIO_ID = "coreaudio:io.github.golovaschukwork.screampad.audio.device.2ch";

export function automaticVirtualOutput(devices: AudioDeviceDto[]): AudioDeviceDto | null {
  const compatible = devices.filter((device) =>
    device.input && device.output && device.configuration === "supported");
  const own = compatible.find((device) => device.id === SCREAM_PAD_AUDIO_ID);
  if (own) return own;
  const virtual = compatible.filter((device) => device.kind === "virtual");
  return virtual.length === 1 ? virtual[0] : null;
}
