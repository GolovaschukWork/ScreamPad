import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../shared/theme/ThemeProvider";
import { resetBackend } from "../test/fakeBindings";
import { setMediaMatches } from "../test/matchMedia";
import App from "./App";

vi.mock("../shared/ipc/bindings", async () => {
  const fake = await import("../test/fakeBindings");
  return { commands: fake.commands };
});

function renderApp() {
  return render(
    <ThemeProvider>
      <App />
    </ThemeProvider>,
  );
}

describe("application shell and navigation", () => {
  beforeEach(() => resetBackend());

  it("opens on the library with a labelled sidebar", async () => {
    renderApp();
    const nav = screen.getByRole("navigation", { name: "Main navigation" });
    const library = within(nav).getByRole("button", { name: "Library" });
    expect(library.getAttribute("aria-current")).toBe("page");
    expect(within(nav).getByRole("button", { name: "Settings" }).getAttribute("aria-current")).toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "Library" })).not.toBeNull();
    expect(screen.getByText("ScreamPad")).not.toBeNull();
  });

  it("lists only pages that exist: boards and audio routing are not shown", () => {
    renderApp();
    expect(screen.queryByText(/boards/i)).toBeNull();
    expect(screen.queryByText(/routing/i)).toBeNull();
    const nav = screen.getByRole("navigation", { name: "Main navigation" });
    expect(within(nav).getAllByRole("button").map((button) => button.textContent)).toEqual(["Library", "Settings"]);
  });

  it("switches pages with the mouse and moves the current-page marker", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).not.toBeNull();
    expect(screen.getByRole("button", { name: "Settings" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("button", { name: "Library" }).getAttribute("aria-current")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Library" }));
    expect(screen.getByRole("heading", { level: 1, name: "Library" })).not.toBeNull();
  });

  it("is fully usable from the keyboard", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Library" }));
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Settings" }));
    await user.keyboard("{Enter}");
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).not.toBeNull();
  });

  it("keeps the labels readable to assistive technology when the sidebar is compact", () => {
    setMediaMatches("(max-width: 719px)", true);
    renderApp();
    const settings = screen.getByRole("button", { name: "Settings" });
    expect(settings).not.toBeNull();
    expect(within(settings).getByText("Settings").classList.contains("sr-only")).toBe(true);
  });

  it("reacts when the window becomes compact", async () => {
    renderApp();
    const label = within(screen.getByRole("button", { name: "Library" })).getByText("Library");
    expect(label.classList.contains("sr-only")).toBe(false);
    setMediaMatches("(max-width: 719px)", true);
    expect(await screen.findByRole("button", { name: "Library" })).not.toBeNull();
    expect(within(screen.getByRole("button", { name: "Library" })).getByText("Library").classList.contains("sr-only")).toBe(true);
  });
});
