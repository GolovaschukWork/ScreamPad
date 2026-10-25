import { XIcon } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatBytes } from "../../shared/format/bytes";
import type { AppError, PlayModeDto, SoundSummary } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { ChoiceSelect } from "@/shared/components/ChoiceSelect";
import { DbSlider } from "@/shared/components/DbSlider";
import { Button } from "@/shared/ui/button";
import { SoundMeta } from "./SoundMeta";

/** Matches the backend's per-sound gain range (ADR 0010). */
const GAIN_MIN_MILLIBELS = -6000;
const GAIN_MAX_MILLIBELS = 600;
const PLAY_MODES: readonly PlayModeDto[] = ["overlap", "restart", "ignore", "toggle"];

type Props = {
  sound: SoundSummary;
  /** Resolves to the error when the change was refused, otherwise `undefined`. */
  onChange: (id: number, gainMillibels: number, playMode: PlayModeDto) => Promise<AppError | null | undefined>;
  onClose: () => void;
};

/** Facts and playback options of the selected sound. */
export function SoundDetails({ sound, onChange, onClose }: Props) {
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

  const channels = sound.channels === 1 ? t("sound.mono") : sound.channels === 2 ? t("sound.stereo") : String(sound.channels);

  return (
    <section aria-label={t("sound.details")} className="space-y-4 rounded-lg border bg-card p-4 text-card-foreground">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-base font-semibold break-words">{sound.displayName}</h2>
          <SoundMeta sound={sound} />
        </div>
        <Button type="button" size="icon" variant="ghost" aria-label={t("sound.closeDetails")} onClick={onClose}>
          <XIcon aria-hidden="true" />
        </Button>
      </div>

      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted-foreground">{t("sound.sampleRate")}</dt>
        <dd className="tabular-nums">{new Intl.NumberFormat(i18n.language).format(sound.sampleRate)} Hz</dd>
        <dt className="text-muted-foreground">{t("sound.channels")}</dt>
        <dd>{channels}</dd>
        <dt className="text-muted-foreground">{t("sound.size")}</dt>
        <dd className="tabular-nums">{sound.sizeBytes === null ? "—" : formatBytes(sound.sizeBytes, i18n.language)}</dd>
      </dl>

      <div className="space-y-3">
        <div className="space-y-1">
          <p className="text-sm font-medium">{t("sound.gain")}</p>
          <DbSlider
            label={t("sound.gain")}
            valueMillibels={sound.gainMillibels}
            minMillibels={GAIN_MIN_MILLIBELS}
            maxMillibels={GAIN_MAX_MILLIBELS}
            locale={i18n.language}
            onCommit={(millibels) => void change(millibels, sound.playMode)}
          />
        </div>
        <div className="space-y-1">
          <label htmlFor={modeId} className="text-sm font-medium">
            {t("sound.playMode")}
          </label>
          <div>
            <ChoiceSelect
              id={modeId}
              value={sound.playMode}
              options={PLAY_MODES.map((mode) => ({ value: mode, label: modeNames[mode] }))}
              className="w-full"
              onChange={(value) => {
                const mode = PLAY_MODES.find((candidate) => candidate === value);
                if (mode) void change(sound.gainMillibels, mode);
              }}
            />
          </div>
        </div>
      </div>

      {problem !== undefined && (
        <p role="alert" className="text-sm text-destructive">
          {problem ? errorText(t, problem) : t("errors.backendUnreachable")}
        </p>
      )}
    </section>
  );
}
