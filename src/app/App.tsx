import { useEffect, useState } from "react";
import { commands, type AppInfo } from "../shared/ipc/bindings";

type State =
  | { status: "loading" }
  | { status: "ready"; info: AppInfo }
  | { status: "error"; message: string };

export default function App() {
  const [state, setState] = useState<State>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    commands
      .appInfo()
      .then((info) => {
        if (!cancelled) setState({ status: "ready", info });
      })
      .catch((error: unknown) => {
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
        {state.status === "loading" && "Connecting to the backend…"}
        {state.status === "ready" && `Backend connected. Version ${state.info.version}.`}
        {state.status === "error" && `Backend unavailable: ${state.message}`}
      </p>
    </main>
  );
}
