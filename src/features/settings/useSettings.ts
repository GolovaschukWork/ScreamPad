import { useCallback, useEffect, useRef, useState } from "react";
import { applyLanguage } from "../../shared/i18n";
import { commands, type AppError, type SettingsDto, type ThemeDto } from "../../shared/ipc/bindings";
import { useTheme } from "../../shared/theme/ThemeProvider";

export type SettingsState =
  | { status: "loading" }
  | { status: "ready"; settings: SettingsDto }
  // `null` means the call itself failed (for example outside the desktop shell), not a typed backend error.
  | { status: "error"; error: AppError | null };

export function useSettings() {
  const [state, setState] = useState<SettingsState>({ status: "loading" });
  // The last failed save; the stored settings stay on screen.
  const [problem, setProblem] = useState<AppError | null | undefined>(undefined);
  const mounted = useRef(true);
  const { setTheme } = useTheme();

  useEffect(() => {
    mounted.current = true;
    void (async () => {
      try {
        const result = await commands.getSettings();
        if (!mounted.current) return;
        setState(result.status === "ok" ? { status: "ready", settings: result.data } : { status: "error", error: result.error });
      } catch {
        if (mounted.current) setState({ status: "error", error: null });
      }
    })();
    return () => {
      mounted.current = false;
    };
  }, []);

  /** Stores the settings. Resolves to `true` once they are stored and applied, `false` when the save failed (the error is
   *  in `problem` and the previous settings stay on screen). */
  const save = useCallback(async (language: string, masterGainMillibels: number, theme: ThemeDto): Promise<boolean> => {
    setProblem(undefined);
    try {
      const result = await commands.updateSettings(language, masterGainMillibels, theme);
      if (!mounted.current) return false;
      if (result.status === "error") {
        setProblem(result.error);
        return false;
      }
      setState({ status: "ready", settings: result.data });
      // The stored value is the truth: the provider only mirrors it.
      setTheme(result.data.theme);
      await applyLanguage(result.data.language);
      return true;
    } catch {
      if (mounted.current) setProblem(null);
      return false;
    }
  }, [setTheme]);

  return { state, problem, save };
}
