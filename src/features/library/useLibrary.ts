import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type AppError, type SoundSummary } from "../../shared/ipc/bindings";

export type ListState =
  | { status: "loading" }
  | { status: "ready"; sounds: SoundSummary[] }
  // `null` means the call itself failed (for example outside the desktop shell), not a typed backend error.
  | { status: "error"; error: AppError | null };

export type ImportState =
  | { status: "idle" }
  | { status: "running" }
  | { status: "done"; name: string; alreadyInLibrary: boolean }
  | { status: "error"; error: AppError | null };

export function useLibrary() {
  const [list, setList] = useState<ListState>({ status: "loading" });
  const [importState, setImportState] = useState<ImportState>({ status: "idle" });
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

  const importSound = useCallback(async () => {
    setImportState({ status: "running" });
    try {
      const result = await commands.importSound();
      if (!mounted.current) return;
      if (result.status === "error") {
        setImportState({ status: "error", error: result.error });
        return;
      }
      if (result.data === null) {
        // The user closed the dialog without choosing a file.
        setImportState({ status: "idle" });
        return;
      }
      setImportState({
        status: "done",
        name: result.data.sound.displayName,
        alreadyInLibrary: result.data.alreadyInLibrary,
      });
      await refresh();
    } catch {
      if (mounted.current) setImportState({ status: "error", error: null });
    }
  }, [refresh]);

  return { list, importState, importSound };
}
