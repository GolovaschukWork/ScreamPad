/** The theme choices of the application. `system` follows the operating system's color scheme. */
export const THEMES = ["system", "dark", "light"] as const;
export type Theme = (typeof THEMES)[number];
export type ResolvedTheme = "dark" | "light";

/** Parses a stored value; anything unknown is `undefined`, so the caller decides the fallback. */
export function parseTheme(value: string): Theme | undefined {
  return THEMES.find((theme) => theme === value);
}

export function resolveTheme(theme: Theme, systemPrefersDark: boolean): ResolvedTheme {
  if (theme === "system") return systemPrefersDark ? "dark" : "light";
  return theme;
}

/** The part of `document.documentElement` that theming touches. */
export type ThemeRoot = {
  classList: { add(token: string): void; remove(token: string): void };
  style: { colorScheme: string };
};

/** Puts exactly one of the classes `dark` and `light` on the root and sets the matching native color scheme. */
export function applyTheme(root: ThemeRoot, theme: Theme, systemPrefersDark: boolean): ResolvedTheme {
  const resolved = resolveTheme(theme, systemPrefersDark);
  root.classList.remove(resolved === "dark" ? "light" : "dark");
  root.classList.add(resolved);
  root.style.colorScheme = resolved;
  return resolved;
}
