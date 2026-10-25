import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import "./app/index.css";
import { ThemeProvider } from "./shared/theme/ThemeProvider";
import { applyLanguage, initI18n } from "./shared/i18n";
import { commands } from "./shared/ipc/bindings";
import type { Theme } from "./shared/theme/theme";

initI18n();

/** Applies the stored language before the first render so the interface does not flash in the wrong language, and
 *  returns the stored theme. Until this resolves nothing is rendered and the page already follows the system color
 *  scheme through CSS. If the settings cannot be read (damaged row, or running outside the desktop shell) the system
 *  language and theme stay. */
async function applyStoredSettings(): Promise<Theme> {
  try {
    const result = await commands.getSettings();
    if (result.status === "ok") {
      await applyLanguage(result.data.language);
      return result.data.theme;
    }
  } catch {
    // Keep the language and theme chosen from the system.
  }
  return "system";
}

void applyStoredSettings().then((theme) => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ThemeProvider initialTheme={theme}>
        <App />
      </ThemeProvider>
    </React.StrictMode>,
  );
});
