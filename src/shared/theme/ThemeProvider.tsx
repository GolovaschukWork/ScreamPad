import { createContext, useContext, useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { applyTheme, type Theme } from "./theme";

const DARK_QUERY = "(prefers-color-scheme: dark)";

type ThemeContextValue = {
  theme: Theme;
  setTheme: (theme: Theme) => void;
};

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

/** Holds the chosen theme and keeps the root element's class in step with it and with the system color scheme.
 *  It stores nothing: the application settings are the only persistent source of the theme. */
export function ThemeProvider({ initialTheme = "system", children }: { initialTheme?: Theme; children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  // Before the first paint, and again whenever the choice or the system scheme changes.
  useLayoutEffect(() => {
    const query = window.matchMedia(DARK_QUERY);
    const apply = () => applyTheme(document.documentElement, theme, query.matches);
    apply();
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, [theme]);

  const value = useMemo(() => ({ theme, setTheme }), [theme]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used inside ThemeProvider");
  return value;
}
