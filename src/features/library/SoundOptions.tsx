import { useState } from "react";
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
    <details>
      <summary aria-label={t("sound.options", { name: sound.displayName })}>{t("sound.optionsShort")}</summary>
      <p>{t("sound.gain")}</p>
      <DbSlider
        label={t("sound.gain")}
        valueMillibels={sound.gainMillibels}
        minMillibels={GAIN_MIN_MILLIBELS}
        maxMillibels={GAIN_MAX_MILLIBELS}
        locale={i18n.language}
        onCommit={(millibels) => void change(millibels, sound.playMode)}
      />
      <p>
        <label>
          {t("sound.playMode")}{" "}
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
        </label>
      </p>
      <p role="alert">{problem !== undefined && (problem ? errorText(t, problem) : t("errors.backendUnreachable"))}</p>
    </details>
  );
}
