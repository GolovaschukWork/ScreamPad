import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { backend, commands, emitPlaying, resetBackend, sound } from "../../test/fakeBindings";
import { LibraryPage } from "./LibraryPage";
import { resetLibraryViewState } from "./libraryViewStore";

vi.mock("../../shared/ipc/bindings", async () => {
  const fake = await import("../../test/fakeBindings");
  return { commands: fake.commands, events: fake.events };
});
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const SOUNDS = [
  sound({ id: 1, displayName: "Airhorn", durationMs: 1500 }),
  sound({ id: 2, displayName: "Rimshot", durationMs: 800 }),
  sound({ id: 3, displayName: "Гудок", durationMs: 2500 }),
];

function names(): string[] {
  return screen
    .queryAllByRole("button", { name: /^Play / })
    .map((button) => (button.getAttribute("aria-label") ?? "").replace(/^Play /, ""));
}

/** The per-sound stop buttons (icon only), not the toolbar's "Stop all". */
function stopButtons(): HTMLElement[] {
  return screen.queryAllByRole("button", { name: /^Stop / }).filter((button) => button.textContent === "");
}

async function ready() {
  await screen.findByRole("list", { name: "Sounds in your library" });
}

describe("library page", () => {
  beforeEach(() => {
    resetBackend();
    resetLibraryViewState();
    backend.sounds = [...SOUNDS];
  });

  it("shows loading, then the real library with its count", async () => {
    render(<LibraryPage />);
    expect(screen.getByRole("status").textContent).toContain("Opening the library");
    await ready();
    expect(screen.getByText("3 sounds")).not.toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "Library" })).not.toBeNull();
    expect(commands.listSounds).toHaveBeenCalledTimes(1);
  });

  it("offers an import button when the library is empty", async () => {
    const user = userEvent.setup();
    backend.sounds = [];
    render(<LibraryPage />);
    expect(await screen.findByText("Your library is empty")).not.toBeNull();
    expect(screen.queryByRole("toolbar")).toBeNull();
    const buttons = screen.getAllByRole("button", { name: "Import sound…" });
    await user.click(buttons[buttons.length - 1] as HTMLElement);
    expect(commands.importSound).toHaveBeenCalledTimes(1);
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("reports an unreadable library with a retry that works", async () => {
    const user = userEvent.setup();
    backend.failReadsWith = { kind: "storageBusy" };
    render(<LibraryPage />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The library is busy");
    backend.failReadsWith = null;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await ready();
    expect(names()).toHaveLength(3);
  });

  it("starts in the grid, switches to the list with the same sounds, and remembers the choice for the session", async () => {
    const user = userEvent.setup();
    const first = render(<LibraryPage />);
    await ready();
    const list = screen.getByRole("list", { name: "Sounds in your library" });
    expect(list.getAttribute("data-view")).toBe("grid");
    expect(screen.getByRole("button", { name: "Grid view" }).getAttribute("aria-pressed")).toBe("true");
    const inGrid = names();

    await user.click(screen.getByRole("button", { name: "List view" }));
    expect(screen.getByRole("list", { name: "Sounds in your library" }).getAttribute("data-view")).toBe("list");
    expect(names()).toEqual(inGrid);

    // Leaving the page and coming back (as through Settings) keeps the view.
    first.unmount();
    render(<LibraryPage />);
    await ready();
    expect(screen.getByRole("list", { name: "Sounds in your library" }).getAttribute("data-view")).toBe("list");
  });

  it("never turns both views off", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    await user.click(screen.getByRole("button", { name: "Grid view" }));
    expect(screen.getByRole("list", { name: "Sounds in your library" }).getAttribute("data-view")).toBe("grid");
    expect(screen.getByRole("button", { name: "Grid view" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("filters by name in both views and can clear the search", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    await user.type(screen.getByRole("searchbox", { name: "Search sounds" }), "RIM");
    expect(names()).toEqual(["Rimshot"]);
    await user.click(screen.getByRole("button", { name: "List view" }));
    expect(names()).toEqual(["Rimshot"]);

    await user.clear(screen.getByRole("searchbox", { name: "Search sounds" }));
    await user.type(screen.getByRole("searchbox", { name: "Search sounds" }), "гуд");
    expect(names()).toEqual(["Гудок"]);

    await user.clear(screen.getByRole("searchbox", { name: "Search sounds" }));
    await user.type(screen.getByRole("searchbox", { name: "Search sounds" }), "nothing");
    expect(screen.getByText("No sounds match “nothing”")).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Clear search" }));
    expect(names()).toHaveLength(3);
  });

  it("sorts by name, then by duration, then by recency", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    expect(names()).toEqual(["Airhorn", "Rimshot", "Гудок"]);

    for (const [option, expected] of [
      ["Duration", ["Rimshot", "Airhorn", "Гудок"]],
      ["Recently added", ["Гудок", "Rimshot", "Airhorn"]],
      ["Name", ["Airhorn", "Rimshot", "Гудок"]],
    ] as const) {
      await user.click(screen.getByRole("combobox", { name: "Sort by" }));
      await user.click(await screen.findByRole("option", { name: option }));
      expect(names()).toEqual([...expected]);
    }
  });

  it("plays through the backend and shows the same playing state in the grid and the list", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    expect(stopButtons()).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Play Rimshot" }));
    expect(commands.playSound).toHaveBeenCalledWith(2);
    // Starting a sound does not by itself claim it plays: only the backend's event does.
    expect(screen.queryByRole("button", { name: "Stop Rimshot" })).toBeNull();

    act(() => emitPlaying([2]));
    expect(await screen.findByRole("button", { name: "Stop Rimshot" })).not.toBeNull();
    expect(screen.getAllByText("Playing")).toHaveLength(1);
    expect(screen.getAllByRole("listitem").filter((item) => item.getAttribute("data-playing") === "true")).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "List view" }));
    expect(screen.getByRole("button", { name: "Stop Rimshot" })).not.toBeNull();
    expect(screen.getAllByText("Playing")).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Stop Rimshot" }));
    expect(commands.stopSound).toHaveBeenCalledWith(2);
    act(() => emitPlaying([]));
    await waitFor(() => expect(stopButtons()).toHaveLength(0));
    expect(screen.queryByText("Playing")).toBeNull();
  });

  it("learns what is already playing when the page opens", async () => {
    backend.playing = [1, 3];
    render(<LibraryPage />);
    await ready();
    expect(await screen.findByRole("button", { name: "Stop Airhorn" })).not.toBeNull();
    expect(screen.getByRole("button", { name: "Stop Гудок" })).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Stop Rimshot" })).toBeNull();
  });

  it("stops everything with Stop all", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    await user.click(screen.getByRole("button", { name: "Stop all" }));
    expect(commands.stopAllSounds).toHaveBeenCalledTimes(1);
  });

  it("can be played from the keyboard in both views", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    for (const view of ["Grid view", "List view"]) {
      await user.click(screen.getByRole("button", { name: view }));
      commands.playSound.mockClear();
      screen.getByRole("button", { name: "Play Airhorn" }).focus();
      await user.keyboard("{Enter}");
      expect(commands.playSound).toHaveBeenCalledWith(1);
      await user.keyboard(" ");
      expect(commands.playSound).toHaveBeenCalledTimes(2);
    }
  });

  it("does not offer to play a sound whose file is missing", async () => {
    backend.sounds = [sound({ id: 7, displayName: "Lost", status: "missing" }), ...SOUNDS];
    render(<LibraryPage />);
    await ready();
    expect((screen.getByRole("button", { name: "Play Lost" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("File missing")).not.toBeNull();
    expect((screen.getByRole("button", { name: "Play Airhorn" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("opens details for a selected sound, saves its options and closes", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    expect(screen.queryByRole("region", { name: "Sound details" })).toBeNull();

    await user.click(screen.getByRole("button", { name: /^Airhorn/ }));
    const details = await screen.findByRole("region", { name: "Sound details" });
    expect(within(details).getByRole("heading", { name: "Airhorn" })).not.toBeNull();
    expect(within(details).getByText(/48,000 Hz/)).not.toBeNull();

    await user.click(within(details).getByRole("combobox", { name: "When played again" }));
    await user.click(await screen.findByRole("option", { name: "Stop if playing" }));
    await waitFor(() => expect(commands.setSoundOptions).toHaveBeenCalledWith(1, 0, "toggle"));
    expect(backend.sounds[0]?.playMode).toBe("toggle");

    await user.click(within(details).getByRole("button", { name: "Close details" }));
    expect(screen.queryByRole("region", { name: "Sound details" })).toBeNull();
  });

  it("hides the details of a sound that a search filtered out", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();
    await user.click(screen.getByRole("button", { name: /^Airhorn/ }));
    await screen.findByRole("region", { name: "Sound details" });
    await user.type(screen.getByRole("searchbox", { name: "Search sounds" }), "rim");
    expect(screen.queryByRole("region", { name: "Sound details" })).toBeNull();
  });

  it("tells the user how an import went", async () => {
    const user = userEvent.setup();
    render(<LibraryPage />);
    await ready();

    backend.importResult = { sound: sound({ id: 9, displayName: "Fresh" }), alreadyInLibrary: false };
    await user.click(screen.getByRole("button", { name: "Import sound…" }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith("Imported “Fresh”."));

    backend.importResult = { sound: sound({ id: 9, displayName: "Fresh" }), alreadyInLibrary: true };
    await user.click(screen.getByRole("button", { name: "Import sound…" }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith("“Fresh” is already in your library."));

    backend.importError = { kind: "importUnsupportedFormat" };
    await user.click(screen.getByRole("button", { name: "Import sound…" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("This is not a supported audio file."));
  });

  it("shows a typed playback error from the backend", async () => {
    const user = userEvent.setup();
    commands.playSound.mockResolvedValueOnce({ status: "error", error: { kind: "audioUnavailable" } });
    render(<LibraryPage />);
    await ready();
    await user.click(screen.getByRole("button", { name: "Play Airhorn" }));
    expect((await screen.findByRole("alert")).textContent).toContain("no audio output");
  });
});
