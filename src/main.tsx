import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import "./app/index.css";
import { ThemeProvider } from "./shared/theme/ThemeProvider";
import { applyLanguage, initI18n } from "./shared/i18n";
import { commands } from "./shared/ipc/bindings";

initI18n();

/** Applies the stored language before the first render so the interface does not flash in the wrong language. If the
 *  settings cannot be read (damaged row, or running outside the desktop shell) the system language stays. */
async function applyStoredLanguage(): Promise<void> {
  try {
    const result = await commands.getSettings();
    if (result.status === "ok") await applyLanguage(result.data.language);
  } catch {
    // Keep the language chosen from the system.
  }
}

void applyStoredLanguage().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ThemeProvider>
        <App />
      </ThemeProvider>
    </React.StrictMode>,
  );
});
