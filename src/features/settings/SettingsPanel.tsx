import { useId } from "react";
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
  const languageId = useId();
  const { state, problem, save } = useSettings();

  const failureText = (error: AppError | null) => (error ? errorText(t, error) : t("errors.backendUnreachable"));
  const languageNames: Record<string, string> = {
    system: t("settings.languageSystem"),
    en: t("settings.languageEn"),
    uk: t("settings.languageUk"),
  };

  return (
    <section className="card" aria-label={t("settings.title")}>
      <h2>{t("settings.title")}</h2>
      {state.status === "loading" && <p className="notice" role="status">{t("settings.loading")}</p>}
      {state.status === "error" && <p className="notice notice-error" role="alert">{failureText(state.error)}</p>}
      {state.status === "ready" && (
        <>
          <div className="field">
            <label htmlFor={languageId}>{t("settings.language")}</label>
            <select
                id={languageId}
                value={state.settings.language}
                onChange={(event) => void save(event.currentTarget.value, state.settings.masterGainMillibels)}
              >
                {Object.entries(languageNames).map(([code, name]) => (
                  <option key={code} value={code}>
                    {name}
                  </option>
                ))}
            </select>
          </div>
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
      <p className="notice notice-error" role="alert">{problem !== undefined && failureText(problem)}</p>
    </section>
  );
}
