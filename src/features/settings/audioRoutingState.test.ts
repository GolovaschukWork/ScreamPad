import { describe, expect, it } from "vitest";
import type { AudioDeviceDto } from "../../shared/ipc/bindings";
import { routingState, selectableDevices, type SelectedDevice } from "./audioRoutingState";

const selected: SelectedDevice = { id: "virtual", name: "Virtual Output" };
const device = (overrides: Partial<AudioDeviceDto> = {}): AudioDeviceDto => ({
  id: "virtual", name: "Virtual Output", kind: "virtual", input: true, output: true,
  configuration: "supported", ...overrides,
});
const ready = (devices: AudioDeviceDto[]) => ({ status: "ready" as const, result: { devices, incomplete: false } });

describe("audio routing states", () => {
  it("distinguishes scanning, failed, setup required, candidate and selected", () => {
    expect(routingState({ status: "scanning" }, null)).toBe("scanning");
    expect(routingState({ status: "failed" }, null)).toBe("discoveryFailed");
    expect(routingState(ready([]), null)).toBe("setupRequired");
    expect(routingState(ready([device()]), null)).toBe("candidateDetected");
    expect(routingState(ready([device()]), selected)).toBe("deviceSelected");
  });

  it("reports disappearance and unsupported configuration without selecting physical devices", () => {
    expect(routingState(ready([]), selected)).toBe("deviceUnavailable");
    expect(routingState(ready([device({ configuration: "unsupported" })]), null)).toBe("unsupportedConfiguration");
    expect(routingState(ready([device({ configuration: "unsupported" })]), selected)).toBe("unsupportedConfiguration");
    expect(selectableDevices([device({ kind: "physical" }), device({ id: "unknown", kind: "unknown" })]).map((item) => item.id)).toEqual(["unknown"]);
  });
});
