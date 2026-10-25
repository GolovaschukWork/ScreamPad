import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../../shared/theme/ThemeProvider";
import { backend, commands, resetBackend } from "../../test/fakeBindings";
import { SettingsPage } from "./SettingsPage";

vi.mock("../../shared/ipc/bindings", async () => {
  const fake = await import("../../test/fakeBindings");
  return { commands: fake.commands, events: fake.events };
});

function renderPage(initialTheme: "system" | "dark" | "light" = "system") {
  return render(
    <ThemeProvider initialTheme={initialTheme}>
      <SettingsPage />
    </ThemeProvider>,
  );
}

async function choose(user: ReturnType<typeof userEvent.setup>, label: string, option: string) {
  await user.click(await screen.findByRole("combobox", { name: label }));
  await user.click(await screen.findByRole("option", { name: option }));
}

// Base UI keeps a slider thumb `visibility: hidden` until it has measured its track, which jsdom cannot do; the slider
// is looked up by its label instead of by role and name (a hidden node has no computed accessible name). The input inside
// is a real, focusable range input with an `aria-label` and `aria-valuetext`.
describe("settings page", () => {
  beforeEach(() => {
    resetBackend();
    commands.updateSettings.mockClear();
  });

  it("shows the stored settings in labelled sections", async () => {
    backend.settings = { language: "uk", masterGainMillibels: -600, theme: "dark" };
    renderPage("dark");
    for (const name of ["General", "Appearance", "Audio", "About"]) {
      expect(await screen.findByRole("region", { name })).not.toBeNull();
    }
    expect(screen.getByRole("combobox", { name: "Language" }).textContent).toContain("Українська");
    expect(screen.getByRole("combobox", { name: "Theme" }).textContent).toContain("Dark");
    expect(screen.getByLabelText("Master volume").getAttribute("aria-valuetext")).toBe("-6 dB");
  });

  it("previews a theme at once, then stores it", async () => {
    const user = userEvent.setup();
    renderPage();
    await choose(user, "Theme", "Light");
    expect(document.documentElement.classList.contains("light")).toBe(true);
    await waitFor(() => expect(commands.updateSettings).toHaveBeenCalledWith("system", 0, "light"));
    expect(backend.settings.theme).toBe("light");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("puts the previous theme back and says so when the save fails", async () => {
    const user = userEvent.setup();
    backend.settings.theme = "dark";
    backend.failUpdateWith = { kind: "storageBusy" };
    renderPage("dark");
    await choose(user, "Theme", "Light");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The library is busy");
    await waitFor(() => expect(document.documentElement.classList.contains("dark")).toBe(true));
    expect(document.documentElement.classList.contains("light")).toBe(false);
    expect(screen.getByRole("combobox", { name: "Theme" }).textContent).toContain("Dark");
    expect(backend.settings.theme).toBe("dark");
  });

  it("changes the language only after the save succeeded", async () => {
    const user = userEvent.setup();
    let release: () => void = () => {};
    backend.holdUpdate = new Promise<void>((resolve) => {
      release = resolve;
    });
    renderPage();
    await choose(user, "Language", "Українська");
    expect(screen.getByRole("heading", { name: "General" })).not.toBeNull();
    expect(document.documentElement.lang).toBe("en");
    await act(async () => release());
    await waitFor(() => expect(document.documentElement.lang).toBe("uk"));
    expect(commands.updateSettings).toHaveBeenCalledWith("uk", 0, "system");
  });

  it("keeps the old language when the save fails", async () => {
    const user = userEvent.setup();
    backend.failUpdateWith = { kind: "storageIo" };
    renderPage();
    await choose(user, "Language", "Українська");
    expect(await screen.findByRole("alert")).not.toBeNull();
    expect(document.documentElement.lang).toBe("en");
    expect(backend.settings.language).toBe("system");
  });

  it("stores the master volume after the slider stops moving, from the keyboard", async () => {
    const user = userEvent.setup();
    renderPage();
    const slider = await screen.findByLabelText("Master volume");
    slider.focus();
    await user.keyboard("{ArrowLeft}{ArrowLeft}{ArrowLeft}");
    expect(screen.getByLabelText("Master volume").getAttribute("aria-valuetext")).toBe("-3 dB");
    await waitFor(() => expect(commands.updateSettings).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(commands.updateSettings).toHaveBeenCalledWith("system", -300, "system");
  });

  it("does not lose a volume change when the user leaves the page right away", async () => {
    const user = userEvent.setup();
    const { unmount } = renderPage();
    const slider = await screen.findByLabelText("Master volume");
    slider.focus();
    await user.keyboard("{ArrowLeft}");
    expect(commands.updateSettings).not.toHaveBeenCalled();
    unmount();
    expect(commands.updateSettings).toHaveBeenCalledWith("system", -100, "system");
  });

  it("reports unreadable settings instead of inventing defaults", async () => {
    backend.failReadsWith = { kind: "storageCorrupt" };
    renderPage();
    expect((await screen.findByRole("alert")).textContent).toContain("damaged");
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("shows the application version and a neutral license statement", async () => {
    renderPage();
    const about = await screen.findByRole("region", { name: "About" });
    expect(await within(about).findByText("0.1.0")).not.toBeNull();
    expect(within(about).getByText("Open source, license to be announced")).not.toBeNull();
    expect(within(about).queryByText(/GPL|MIT|Apache/)).toBeNull();
  });

  it("says when the version cannot be read", async () => {
    backend.appInfoFails = true;
    renderPage();
    const about = await screen.findByRole("region", { name: "About" });
    expect(await within(about).findByText("Unavailable")).not.toBeNull();
  });
});
