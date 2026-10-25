import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type AppError, type PlayModeDto, type SoundSummary } from "../../shared/ipc/bindings";

export type ListState =
  | { status: "loading" }
  | { status: "ready"; sounds: SoundSummary[] }
  // `null` means the call itself failed (for example outside the desktop shell), not a typed backend error.
  | { status: "error"; error: AppError | null };

/** What an import attempt came to. */
export type ImportResult =
  | { kind: "cancelled" }
  | { kind: "done"; name: string; alreadyInLibrary: boolean }
  | { kind: "error"; error: AppError | null };

export function useLibrary() {
  const [list, setList] = useState<ListState>({ status: "loading" });
  const [importing, setImporting] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const result = await commands.listSounds();
      if (!mounted.current) return;
      setList(
        result.status === "ok" ? { status: "ready", sounds: result.data } : { status: "error", error: result.error },
      );
    } catch {
      if (mounted.current) setList({ status: "error", error: null });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** Retries loading after a failure, showing the loading state again. */
  const reload = useCallback(async () => {
    setList({ status: "loading" });
    await refresh();
  }, [refresh]);

  const importSound = useCallback(async (): Promise<ImportResult> => {
    setImporting(true);
    try {
      const result = await commands.importSound();
      if (result.status === "error") return { kind: "error", error: result.error };
      // The user closed the dialog without choosing a file.
      if (result.data === null) return { kind: "cancelled" };
      await refresh();
      return { kind: "done", name: result.data.sound.displayName, alreadyInLibrary: result.data.alreadyInLibrary };
    } catch {
      return { kind: "error", error: null };
    } finally {
      if (mounted.current) setImporting(false);
    }
  }, [refresh]);

  const setOptions = useCallback(
    async (id: number, gainMillibels: number, playMode: PlayModeDto): Promise<AppError | null | undefined> => {
      try {
        const result = await commands.setSoundOptions(id, gainMillibels, playMode);
        if (result.status === "error") return result.error;
        if (mounted.current) {
          setList((current) =>
            current.status === "ready"
              ? { status: "ready", sounds: current.sounds.map((sound) => (sound.id === id ? result.data : sound)) }
              : current,
          );
        }
        return undefined;
      } catch {
        return null;
      }
    },
    [],
  );

  return { list, importing, importSound, reload, setOptions };
}
