import { useCallback, useEffect, useRef, useState } from "react";
import { applyLanguage } from "../../shared/i18n";
import { commands, type AppError, type SettingsDto } from "../../shared/ipc/bindings";

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

  const save = useCallback(async (language: string, masterGainMillibels: number) => {
    setProblem(undefined);
    try {
      const result = await commands.updateSettings(language, masterGainMillibels);
      if (!mounted.current) return;
      if (result.status === "error") {
        setProblem(result.error);
        return;
      }
      setState({ status: "ready", settings: result.data });
      await applyLanguage(result.data.language);
    } catch {
      if (mounted.current) setProblem(null);
    }
  }, []);

  return { state, problem, save };
}
