import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type AppError, type AudioStatus } from "../../shared/ipc/bindings";

export type AudioState =
  | { status: "loading" }
  | { status: "ready"; audio: AudioStatus }
  // `null` means the call itself failed (for example outside the desktop shell), not a typed backend error.
  | { status: "error"; error: AppError | null };

/** How often the audio output state is refreshed while the window is visible. */
const REFRESH_MS = 2000;

export function useAudio() {
  const [audio, setAudio] = useState<AudioState>({ status: "loading" });
  // `undefined`: no problem; `null`: the call itself failed; otherwise a typed backend error.
  const [problem, setProblem] = useState<AppError | null | undefined>(undefined);
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const result = await commands.audioStatus();
      if (!mounted.current) return;
      setAudio(
        result.status === "ok" ? { status: "ready", audio: result.data } : { status: "error", error: result.error },
      );
    } catch {
      if (mounted.current) setAudio({ status: "error", error: null });
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, REFRESH_MS);
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
    };
  }, [refresh]);

  const run = useCallback(
    async (action: () => Promise<{ status: "ok" } | { status: "error"; error: AppError }>) => {
      setProblem(undefined);
      try {
        const result = await action();
        if (mounted.current && result.status === "error") setProblem(result.error);
      } catch {
        if (mounted.current) setProblem(null);
      }
      void refresh();
    },
    [refresh],
  );

  const play = useCallback((id: number) => run(() => commands.playSound(id)), [run]);
  const stopAll = useCallback(() => run(() => commands.stopAllSounds()), [run]);
  const retry = useCallback(() => run(() => commands.retryAudio()), [run]);

  return { audio, problem, play, stopAll, retry };
}
