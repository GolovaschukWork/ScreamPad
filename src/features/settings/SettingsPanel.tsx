import { useTranslation } from "react-i18next";
import type { AppError } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { DbSlider } from "./DbSlider";
import { useSettings } from "./useSettings";

/** Matches the backend's master gain range (ADR 0010). */
const MASTER_MIN_MILLIBELS = -6000;
const MASTER_MAX_MILLIBELS = 0;

export function SettingsPanel() {
  const { t, i18n } = useTranslation();
  const { state, problem, save } = useSettings();

  const failureText = (error: AppError | null) => (error ? errorText(t, error) : t("errors.backendUnreachable"));
  const languageNames: Record<string, string> = {
    system: t("settings.languageSystem"),
    en: t("settings.languageEn"),
    uk: t("settings.languageUk"),
  };

  return (
    <section aria-label={t("settings.title")}>
      <h2>{t("settings.title")}</h2>
      {state.status === "loading" && <p role="status">{t("settings.loading")}</p>}
      {state.status === "error" && <p role="alert">{failureText(state.error)}</p>}
      {state.status === "ready" && (
        <>
          <p>
            <label>
              {t("settings.language")}{" "}
              <select
                value={state.settings.language}
                onChange={(event) => void save(event.currentTarget.value, state.settings.masterGainMillibels)}
              >
                {Object.entries(languageNames).map(([code, name]) => (
                  <option key={code} value={code}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
          </p>
          <DbSlider
            label={t("settings.masterVolume")}
            valueMillibels={state.settings.masterGainMillibels}
            minMillibels={MASTER_MIN_MILLIBELS}
            maxMillibels={MASTER_MAX_MILLIBELS}
            locale={i18n.language}
            onCommit={(millibels) => void save(state.settings.language, millibels)}
          />
        </>
      )}
      <p role="alert">{problem !== undefined && failureText(problem)}</p>
    </section>
  );
}
