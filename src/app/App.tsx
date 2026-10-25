import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type AppError, type LibraryStats } from "../shared/ipc/bindings";
import { errorText } from "../shared/ipc/errors";

type State =
  | { status: "loading" }
  | { status: "ready"; stats: LibraryStats }
  // `null` means the call itself failed (for example outside the desktop shell), not a typed backend error.
  | { status: "error"; error: AppError | null };

export default function App() {
  const { t } = useTranslation();
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
            : { status: "error", error: result.error },
        );
      })
      .catch(() => {
        if (!cancelled) setState({ status: "error", error: null });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main>
      <h1>ScreamPad</h1>
      <p role="status">
        {state.status === "loading" && t("library.opening")}
        {state.status === "ready" &&
          t("library.ready", { count: state.stats.soundCount, schema: state.stats.schemaVersion })}
        {state.status === "error" && (state.error ? errorText(t, state.error) : t("errors.backendUnreachable"))}
      </p>
    </main>
  );
}
