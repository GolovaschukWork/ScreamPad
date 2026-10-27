import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AudioDeviceDto, AudioDevicesDto } from "../../shared/ipc/bindings";
import { backend, commands, resetBackend } from "../../test/fakeBindings";
import { AudioRouting } from "./AudioRouting";
import { setSelectedDevice, setSelectedMicrophone, setSelectedMonitor } from "./audioRoutingState";

const opener = vi.hoisted(() => ({ openUrl: vi.fn(async (_url: string) => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => opener);

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
    setSelectedMicrophone(null);
    setSelectedMonitor(null);
    commands.listAudioDevices.mockReset();
    commands.listAudioDevices.mockImplementation(async () => ({ status: "ok", data: backend.audioDevices }));
    commands.startCallRouting.mockClear();
    commands.stopCallRouting.mockClear();
    commands.openAudioInstaller.mockClear();
    commands.setLocalMonitorOutput.mockClear();
    opener.openUrl.mockClear();
  });

  it("keeps local playback available through setup required, scanning, and failure", async () => {
    const user = userEvent.setup();
    const pending = new Promise<never>(() => {});
    commands.listAudioDevices.mockImplementationOnce(() => pending);
    render(<AudioRouting />);
    expect(screen.getByRole("status").textContent).toContain("Scanning");
    expect(screen.getByText(/Sounds play locally even without/)).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Refresh Devices" }));
    expect((await screen.findByRole("status")).textContent).toContain("Setup required");
    backend.deviceDiscoveryError = true;
    commands.listAudioDevices.mockImplementationOnce(async () => ({ status: "error", error: { kind: "deviceDiscoveryFailed" } }));
    await user.click(screen.getByRole("button", { name: "Refresh Devices" }));
    expect((await screen.findByRole("status")).textContent).toContain("Device discovery failed");
    expect(screen.getByText(/Sounds play locally even without/)).not.toBeNull();
  });

  it("allows keyboard selection of unknown outputs, keeps selection in session, and reports a missing device", async () => {
    const user = userEvent.setup();
    backend.audioDevices = result([device({ kind: "unknown" })]);
    const page = render(<AudioRouting />);
    const group = await screen.findByText("Choose a virtual device for calls");
    expect(group).not.toBeNull();
    const select = screen.getByRole("button", { name: "Select Virtual Output" });
    select.focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Virtual Output selected" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("status").textContent).toContain("Virtual device ready");
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

  it("shows when an output has no matching input role", async () => {
    backend.audioDevices = result([device({ input: false })]);
    render(<AudioRouting />);
    expect(await screen.findByText(/Matching input role not detected/)).not.toBeNull();
    expect(screen.getByRole("button", { name: "Select Virtual Output" })).not.toBeNull();
  });

  it("ignores a late earlier refresh response", async () => {
    const user = userEvent.setup();
    let resolveFirst: (value: { status: "ok"; data: AudioDevicesDto }) => void = () => {};
    commands.listAudioDevices.mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }));
    backend.audioDevices = result([device()]);
    render(<AudioRouting />);
    await user.click(screen.getByRole("button", { name: "Refresh Devices" }));
    expect((await screen.findByRole("status")).textContent).toContain("Virtual device ready");
    await act(async () => resolveFirst({ status: "ok", data: result([]) }));
    expect(screen.getByRole("status").textContent).toContain("Virtual device ready");
    expect(within(screen.getByRole("region", { name: "Audio Routing" })).getByText(/Virtual device for calls: Virtual Output/)).not.toBeNull();
  });

  it("announces scan changes and opens only the fixed publisher URLs from keyboard-accessible links", async () => {
    const user = userEvent.setup();
    backend.audioDevices = result([]);
    render(<AudioRouting />);
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Setup required"));
    expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
    const download = screen.getByRole("link", { name: /Get BlackHole for macOS/ });
    const guide = screen.getByRole("link", { name: /View installation guide/ });
    expect(download.getAttribute("href")).toBe("https://existential.audio/blackhole/");
    expect(guide.getAttribute("href")).toBe("https://github.com/ExistentialAudio/BlackHole/wiki/Installation");
    download.focus();
    await user.keyboard("{Enter}");
    await user.click(guide);
    expect(opener.openUrl.mock.calls).toEqual([
      ["https://existential.audio/blackhole/"],
      ["https://github.com/ExistentialAudio/BlackHole/wiki/Installation"],
    ]);
  });

  it("announces an opener error without navigating the webview", async () => {
    const user = userEvent.setup();
    opener.openUrl.mockRejectedValueOnce(new Error("not allowed"));
    render(<AudioRouting />);
    await user.click(await screen.findByRole("link", { name: /Get BlackHole for macOS/ }));
    expect((await screen.findByRole("alert")).textContent).toContain("could not be opened");
  });

  it("starts and stops routing only with selected devices and reports backend state", async () => {
    const user = userEvent.setup();
    backend.audioDevices = result([
      device(),
      device({ id: "coreaudio:mic", name: "Built-in Microphone", kind: "unknown", input: true, output: false }),
      device({ id: "coreaudio:speakers", name: "Speakers", kind: "unknown", input: false, output: true }),
    ]);
    render(<AudioRouting />);
    await user.click(await screen.findByRole("combobox", { name: "Physical microphone" }));
    await user.click(await screen.findByRole("option", { name: "Built-in Microphone" }));
    await user.click(screen.getByRole("combobox", { name: "Local monitoring output" }));
    await user.click(await screen.findByRole("option", { name: "Speakers" }));
    expect(commands.setLocalMonitorOutput).toHaveBeenCalledWith("coreaudio:speakers");
    await waitFor(() => expect(screen.getByRole("button", { name: "Start Routing" }).hasAttribute("disabled")).toBe(false));
    await user.click(screen.getByRole("button", { name: "Start Routing" }));
    expect(commands.startCallRouting).toHaveBeenCalledWith("coreaudio:mic", "coreaudio:virtual-1");
    expect((await screen.findByRole("status")).textContent).toContain("Routing active");
    await user.click(screen.getByRole("button", { name: "Stop Routing" }));
    expect(commands.stopCallRouting).toHaveBeenCalled();
    expect((await screen.findByRole("status")).textContent).toContain("Virtual device ready");
  });

  it("shows only microphone and monitor choices when the branded route is available", async () => {
    const user = userEvent.setup();
    const branded = "coreaudio:io.github.golovaschukwork.screampad.audio.device.2ch";
    backend.audioDevices = result([
      device({ id: "coreaudio:blackhole", name: "BlackHole 2ch" }),
      device({ id: branded, name: "ScreamPad Audio 2ch", kind: "unknown" }),
      device({ id: "coreaudio:mic", name: "Built-in Microphone", kind: "physical", input: true, output: false }),
      device({ id: "coreaudio:speakers", name: "Speakers", kind: "physical", input: false, output: true }),
    ]);
    render(<AudioRouting />);
    expect(await screen.findByText(/Virtual device for calls: ScreamPad Audio 2ch/)).not.toBeNull();
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /Select BlackHole/ })).toBeNull();
    await user.click(screen.getByRole("combobox", { name: "Physical microphone" }));
    await user.click(await screen.findByRole("option", { name: "Built-in Microphone" }));
    await user.click(screen.getByRole("button", { name: "Start Routing" }));
    expect(commands.startCallRouting).toHaveBeenCalledWith("coreaudio:mic", branded);
  });

  it("reports a missing previous route and uses the sole available virtual device", async () => {
    const user = userEvent.setup();
    setSelectedDevice({ id: "coreaudio:removed", name: "Old Virtual Device" });
    backend.audioDevices = result([
      device(),
      device({ id: "coreaudio:mic", name: "Built-in Microphone", kind: "physical", input: true, output: false }),
    ]);
    render(<AudioRouting />);
    expect(await screen.findByText(/Previously selected device.*Old Virtual Device/)).not.toBeNull();
    expect(screen.getByText(/Virtual device for calls: Virtual Output/)).not.toBeNull();
    await user.click(screen.getByRole("combobox", { name: "Physical microphone" }));
    await user.click(await screen.findByRole("option", { name: "Built-in Microphone" }));
    await user.click(screen.getByRole("button", { name: "Start Routing" }));
    expect(commands.startCallRouting).toHaveBeenCalledWith("coreaudio:mic", "coreaudio:virtual-1");
  });

  it("opens the native Installer chooser only after an explicit setup click", async () => {
    const user = userEvent.setup();
    backend.audioDevices = result([]);
    render(<AudioRouting />);
    await screen.findByRole("button", { name: "Open ScreamPad Audio package in Installer" });
    expect(commands.openAudioInstaller).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Open ScreamPad Audio package in Installer" }));
    expect(commands.openAudioInstaller).toHaveBeenCalledOnce();
    expect(await screen.findByText(/Installer opened/)).not.toBeNull();
  });
});
