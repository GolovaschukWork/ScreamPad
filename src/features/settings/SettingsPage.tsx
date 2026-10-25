import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type AppError, type AppInfo, type ThemeDto } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { useTheme } from "../../shared/theme/ThemeProvider";
import { ChoiceSelect, type ChoiceOption } from "@/shared/components/ChoiceSelect";
import { DbSlider } from "@/shared/components/DbSlider";
import { SettingsRow, SettingsSection } from "./SettingsSection";
import { useSettings } from "./useSettings";

/** Matches the backend's master gain range (ADR 0010). */
const MASTER_MIN_MILLIBELS = -6000;
const MASTER_MAX_MILLIBELS = 0;
const THEMES: readonly ThemeDto[] = ["system", "dark", "light"];
const LANGUAGES = ["system", "en", "uk"] as const;

function useAppInfo(): AppInfo | null | undefined {
  // `undefined` while loading, `null` when the information cannot be read.
  const [info, setInfo] = useState<AppInfo | null | undefined>(undefined);
  useEffect(() => {
    let active = true;
    commands
      .appInfo()
      .then((value) => active && setInfo(value))
      .catch(() => active && setInfo(null));
    return () => {
      active = false;
    };
  }, []);
  return info;
}

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { state, problem, save } = useSettings();
  const { setTheme } = useTheme();
  const info = useAppInfo();
  const languageId = useId();
  const themeId = useId();

  const failureText = (error: AppError | null) => (error ? errorText(t, error) : t("errors.backendUnreachable"));
  const languageOptions: ChoiceOption[] = LANGUAGES.map((code) => ({
    value: code,
    label: {
      system: t("settings.languageSystem"),
      en: t("settings.languageEn"),
      uk: t("settings.languageUk"),
    }[code],
  }));
  const themeOptions: ChoiceOption[] = THEMES.map((theme) => ({
    value: theme,
    label: { system: t("settings.themeSystem"), dark: t("settings.themeDark"), light: t("settings.themeLight") }[theme],
  }));

  if (state.status === "loading") return <p role="status">{t("settings.loading")}</p>;
  if (state.status === "error") return <p role="alert" className="text-destructive">{failureText(state.error)}</p>;
  const settings = state.settings;

  return (
    <div className="max-w-2xl space-y-6">
      {problem !== undefined && (
        <p role="alert" className="rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {failureText(problem)}
        </p>
      )}

      <SettingsSection title={t("settings.general")}>
        <SettingsRow label={t("settings.language")} htmlFor={languageId}>
          <ChoiceSelect
            id={languageId}
            value={settings.language}
            options={languageOptions}
            onChange={(language) => void save(language, settings.masterGainMillibels, settings.theme)}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t("settings.appearance")}>
        <SettingsRow label={t("settings.theme")} htmlFor={themeId} description={t("settings.themeHint")}>
          <ChoiceSelect
            id={themeId}
            value={settings.theme}
            options={themeOptions}
            onChange={(value) => {
              const theme = THEMES.find((candidate) => candidate === value);
              if (!theme) return;
              // Preview at once; the stored value is the truth, so a failed save puts the previous theme back.
              setTheme(theme);
              void save(settings.language, settings.masterGainMillibels, theme).then((saved) => {
                if (!saved) setTheme(settings.theme);
              });
            }}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t("settings.audio")}>
        <SettingsRow label={t("settings.masterVolume")} description={t("settings.masterVolumeHint")}>
          <DbSlider
            label={t("settings.masterVolume")}
            valueMillibels={settings.masterGainMillibels}
            minMillibels={MASTER_MIN_MILLIBELS}
            maxMillibels={MASTER_MAX_MILLIBELS}
            locale={i18n.language}
            onCommit={(millibels) => void save(settings.language, millibels, settings.theme)}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t("settings.about")}>
        <SettingsRow label={t("settings.appName")}>
          <span className="text-sm">{info ? info.name : "ScreamPad"}</span>
        </SettingsRow>
        <SettingsRow label={t("settings.version")}>
          <span className="text-sm tabular-nums">
            {info === undefined ? t("settings.loading") : info === null ? t("settings.versionUnknown") : info.version}
          </span>
        </SettingsRow>
        <SettingsRow label={t("settings.license")}>
          <span className="text-sm">{t("settings.licensePending")}</span>
        </SettingsRow>
      </SettingsSection>
    </div>
  );
}
