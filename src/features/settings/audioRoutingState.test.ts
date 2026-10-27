import { describe, expect, it } from "vitest";
import type { AudioDeviceDto } from "../../shared/ipc/bindings";
import { automaticVirtualOutput, routingState, selectableDevices, type SelectedDevice } from "./audioRoutingState";

const selected: SelectedDevice = { id: "virtual", name: "Virtual Output" };
const device = (overrides: Partial<AudioDeviceDto> = {}): AudioDeviceDto => ({
  id: "virtual", name: "Virtual Output", kind: "virtual", input: true, output: true,
  configuration: "supported", ...overrides,
});
const ready = (devices: AudioDeviceDto[]) => ({ status: "ready" as const, result: { devices, incomplete: false } });

describe("audio routing states", () => {
  it("automatically uses the branded device or a sole known virtual device, never a physical output", () => {
    const branded = device({ id: "coreaudio:io.github.golovaschukwork.screampad.audio.device.2ch", name: "ScreamPad Audio 2ch", kind: "unknown" });
    expect(automaticVirtualOutput([device(), branded])?.id).toBe(branded.id);
    expect(automaticVirtualOutput([device()])?.id).toBe("virtual");
    expect(automaticVirtualOutput([device(), device({ id: "other" })])).toBeNull();
    expect(automaticVirtualOutput([device({ kind: "physical" })])).toBeNull();
    expect(automaticVirtualOutput([device({ kind: "unknown" })])).toBeNull();
    expect(automaticVirtualOutput([device({ input: false })])).toBeNull();
    expect(automaticVirtualOutput([device({ configuration: "unsupported" })])).toBeNull();
  });
  it("distinguishes scanning, failed, setup required, candidate and selected", () => {
    expect(routingState({ status: "scanning" }, null)).toBe("scanning");
    expect(routingState({ status: "failed" }, null)).toBe("discoveryFailed");
    expect(routingState(ready([]), null)).toBe("setupRequired");
    expect(routingState(ready([device({ input: false, kind: "unknown" })]), null)).toBe("setupRequired");
    expect(routingState(ready([device()]), null)).toBe("candidateDetected");
    expect(routingState(ready([device()]), selected)).toBe("deviceSelected");
  });

  it("reports disappearance and unsupported configuration without selecting physical devices", () => {
    expect(routingState(ready([]), selected)).toBe("deviceUnavailable");
    expect(routingState(ready([device({ configuration: "unsupported" })]), null)).toBe("unsupportedConfiguration");
    expect(routingState(ready([device({ configuration: "unsupported" })]), selected)).toBe("unsupportedConfiguration");
    expect(routingState(ready([device({ input: false })]), selected)).toBe("unsupportedConfiguration");
    expect(selectableDevices([device({ kind: "physical" }), device({ id: "unknown", kind: "unknown" })]).map((item) => item.id)).toEqual(["unknown"]);
  });
});
