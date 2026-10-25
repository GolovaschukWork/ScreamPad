import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AudioDeviceDto, AudioDevicesDto } from "../../shared/ipc/bindings";
import { backend, commands, resetBackend } from "../../test/fakeBindings";
import { AudioRouting } from "./AudioRouting";
import { setSelectedDevice } from "./audioRoutingState";

vi.mock("../../shared/ipc/bindings", async () => {
  const fake = await import("../../test/fakeBindings");
  return { commands: fake.commands };
});

const device = (overrides: Partial<AudioDeviceDto> = {}): AudioDeviceDto => ({
  id: "coreaudio:virtual-1", name: "Virtual Output", kind: "virtual", input: true, output: true,
  configuration: "supported", ...overrides,
});

const result = (devices: AudioDeviceDto[]): AudioDevicesDto => ({ devices, incomplete: false });

describe("audio routing onboarding", () => {
  beforeEach(() => {
    resetBackend();
    setSelectedDevice(null);
    commands.listAudioDevices.mockReset();
    commands.listAudioDevices.mockImplementation(async () => ({ status: "ok", data: backend.audioDevices }));
  });

  it("keeps local playback available through setup required, scanning, and failure", async () => {
    const user = userEvent.setup();
    const pending = new Promise<never>(() => {});
    commands.listAudioDevices.mockImplementationOnce(() => pending);
    render(<AudioRouting />);
    expect(screen.getByRole("status").textContent).toContain("Scanning");
    expect(screen.getByText(/Local playback works without/)).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Refresh Devices" }));
    expect((await screen.findByRole("status")).textContent).toContain("Setup required");
    backend.deviceDiscoveryError = true;
    commands.listAudioDevices.mockImplementationOnce(async () => ({ status: "error", error: { kind: "deviceDiscoveryFailed" } }));
    await user.click(screen.getByRole("button", { name: "Refresh Devices" }));
    expect((await screen.findByRole("status")).textContent).toContain("Device discovery failed");
    expect(screen.getByText(/Local playback works without/)).not.toBeNull();
  });

  it("allows keyboard selection of unknown outputs, keeps selection in session, and reports a missing device", async () => {
    const user = userEvent.setup();
    backend.audioDevices = result([device({ kind: "unknown" })]);
    const page = render(<AudioRouting />);
    const group = await screen.findByText("Other outputs — device type unknown");
    expect(group).not.toBeNull();
    const select = screen.getByRole("button", { name: "Select Virtual Output" });
    select.focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Virtual Output selected" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("status").textContent).toContain("Device selected");
    page.unmount();
    backend.audioDevices = result([]);
    render(<AudioRouting />);
    expect((await screen.findByRole("status")).textContent).toContain("Selected device unavailable");
    expect(screen.getByText(/Previously selected device/).textContent).toContain("Virtual Output");
  });

  it("does not offer unsupported or physical outputs for selection", async () => {
    backend.audioDevices = result([device({ configuration: "unsupported" }), device({ id: "physical", name: "Speakers", kind: "physical" })]);
    render(<AudioRouting />);
    expect((await screen.findByRole("status")).textContent).toContain("Unsupported configuration");
    expect(screen.getByRole("button", { name: "Select Virtual Output" }).hasAttribute("disabled")).toBe(true);
    expect(screen.queryByText("Speakers")).toBeNull();
    expect(screen.queryByText(/Routing Active|Discord Verified/)).toBeNull();
  });

  it("ignores a late earlier refresh response", async () => {
    const user = userEvent.setup();
    let resolveFirst: (value: { status: "ok"; data: AudioDevicesDto }) => void = () => {};
    commands.listAudioDevices.mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }));
    backend.audioDevices = result([device()]);
    render(<AudioRouting />);
    await user.click(screen.getByRole("button", { name: "Refresh Devices" }));
    expect((await screen.findByRole("status")).textContent).toContain("Candidate detected");
    await act(async () => resolveFirst({ status: "ok", data: result([]) }));
    expect(screen.getByRole("status").textContent).toContain("Candidate detected");
    expect(within(screen.getByRole("region", { name: "Audio Routing" })).getByText("Virtual Output")).not.toBeNull();
  });

  it("announces scan changes and shows only fixed publisher addresses", async () => {
    backend.audioDevices = result([device()]);
    render(<AudioRouting />);
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Candidate detected"));
    expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
    expect(screen.getByText("https://existential.audio/blackhole/")).not.toBeNull();
    expect(screen.getByText("https://github.com/ExistentialAudio/BlackHole/wiki/Installation")).not.toBeNull();
  });
});
