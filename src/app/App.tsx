import { useEffect, useState } from "react";
import { commands, type LibraryStats } from "../shared/ipc/bindings";
import { describeError } from "../shared/ipc/messages";

type State =
  | { status: "loading" }
  | { status: "ready"; stats: LibraryStats }
  | { status: "error"; message: string };

export default function App() {
  const [state, setState] = useState<State>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    commands
      .libraryStats()
      .then((result) => {
        if (cancelled) return;
        setState(
          result.status === "ok"
            ? { status: "ready", stats: result.data }
            : { status: "error", message: describeError(result.error) },
        );
      })
      .catch((error: unknown) => {
        // The call itself failed (for example outside the desktop shell), not a typed backend error.
        if (!cancelled) {
          setState({ status: "error", message: error instanceof Error ? error.message : String(error) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main>
      <h1>ScreamPad</h1>
      <p role="status">
        {state.status === "loading" && "Opening the library…"}
        {state.status === "ready" &&
          `Library ready: ${state.stats.soundCount} ${state.stats.soundCount === 1 ? "sound" : "sounds"} (schema ${state.stats.schemaVersion}).`}
        {state.status === "error" && state.message}
      </p>
    </main>
  );
}
