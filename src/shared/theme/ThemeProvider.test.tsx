import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { setMediaMatches } from "../../test/matchMedia";
import { ThemeProvider, useTheme } from "./ThemeProvider";

const DARK = "(prefers-color-scheme: dark)";

function Probe() {
  const { theme, setTheme } = useTheme();
  return (
    <>
      <p>theme: {theme}</p>
      <button onClick={() => setTheme("light")}>light</button>
      <button onClick={() => setTheme("system")}>system</button>
    </>
  );
}

function root() {
  return document.documentElement;
}

describe("ThemeProvider", () => {
  it("follows a dark system before anything is chosen", () => {
    setMediaMatches(DARK, true);
    render(<ThemeProvider><Probe /></ThemeProvider>);
    expect(root().classList.contains("dark")).toBe(true);
    expect(root().style.colorScheme).toBe("dark");
  });

  it("follows a light system", () => {
    render(<ThemeProvider><Probe /></ThemeProvider>);
    expect(root().classList.contains("light")).toBe(true);
    expect(root().classList.contains("dark")).toBe(false);
  });

  it("applies the stored theme regardless of the system", () => {
    setMediaMatches(DARK, true);
    render(<ThemeProvider initialTheme="light"><Probe /></ThemeProvider>);
    expect(root().classList.contains("light")).toBe(true);
    expect(root().classList.contains("dark")).toBe(false);
  });

  it("switches live between explicit and system themes", async () => {
    const user = userEvent.setup();
    setMediaMatches(DARK, true);
    render(<ThemeProvider><Probe /></ThemeProvider>);
    await user.click(screen.getByRole("button", { name: "light" }));
    expect(root().classList.contains("light")).toBe(true);
    expect(screen.getByText("theme: light")).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "system" }));
    expect(root().classList.contains("dark")).toBe(true);
  });

  it("tracks the system scheme while the theme is system, and ignores it otherwise", async () => {
    const user = userEvent.setup();
    render(<ThemeProvider><Probe /></ThemeProvider>);
    expect(root().classList.contains("light")).toBe(true);
    act(() => setMediaMatches(DARK, true));
    expect(root().classList.contains("dark")).toBe(true);
    await user.click(screen.getByRole("button", { name: "light" }));
    act(() => setMediaMatches(DARK, true));
    expect(root().classList.contains("light")).toBe(true);
  });

  it("leaves exactly one scheme class on the root", () => {
    root().classList.add("dark", "light");
    render(<ThemeProvider initialTheme="dark"><Probe /></ThemeProvider>);
    expect([...root().classList].filter((name) => name === "dark" || name === "light")).toEqual(["dark"]);
  });
});
