import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppError, PlayModeDto, SoundSummary } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { DbSlider } from "../settings/DbSlider";

/** Matches the backend's per-sound gain range (ADR 0010). */
const GAIN_MIN_MILLIBELS = -6000;
const GAIN_MAX_MILLIBELS = 600;
const PLAY_MODES: readonly PlayModeDto[] = ["overlap", "restart", "ignore", "toggle"];

type Props = {
  sound: SoundSummary;
  /** Resolves to the error when the change was refused, otherwise `undefined`. */
  onChange: (id: number, gainMillibels: number, playMode: PlayModeDto) => Promise<AppError | null | undefined>;
};

export function SoundOptions({ sound, onChange }: Props) {
  const { t, i18n } = useTranslation();
  const modeId = useId();
  const [problem, setProblem] = useState<AppError | null | undefined>(undefined);
  const modeNames: Record<PlayModeDto, string> = {
    overlap: t("sound.modeOverlap"),
    restart: t("sound.modeRestart"),
    ignore: t("sound.modeIgnore"),
    toggle: t("sound.modeToggle"),
  };

  const change = async (gainMillibels: number, playMode: PlayModeDto) => {
    setProblem(await onChange(sound.id, gainMillibels, playMode));
  };

  return (
    <details className="sound-options">
      <summary aria-label={t("sound.options", { name: sound.displayName })}>{t("sound.optionsShort")}</summary>
      <DbSlider
        label={t("sound.gain")}
        valueMillibels={sound.gainMillibels}
        minMillibels={GAIN_MIN_MILLIBELS}
        maxMillibels={GAIN_MAX_MILLIBELS}
        locale={i18n.language}
        onCommit={(millibels) => void change(millibels, sound.playMode)}
      />
      <div className="field">
        <label htmlFor={modeId}>{t("sound.playMode")}</label>
        <select
            value={sound.playMode}
            onChange={(event) => {
              const mode = PLAY_MODES.find((candidate) => candidate === event.currentTarget.value);
              if (mode) void change(sound.gainMillibels, mode);
            }}
          >
            {PLAY_MODES.map((mode) => (
              <option key={mode} value={mode}>
                {modeNames[mode]}
              </option>
            ))}
        </select>
      </div>
      <p className="notice notice-error" role="alert">{problem !== undefined && (problem ? errorText(t, problem) : t("errors.backendUnreachable"))}</p>
    </details>
  );
}
