import { AudioLinesIcon, TriangleAlertIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatDuration } from "../../shared/format/duration";
import type { SoundSummary } from "../../shared/ipc/bindings";

/** Format and duration of a sound, with its warning when the file is missing or damaged. */
export function SoundMeta({ sound }: { sound: SoundSummary }) {
  const { t, i18n } = useTranslation();
  return (
    <span className="flex min-w-0 flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
      <span>{sound.format.toUpperCase()}</span>
      <span aria-hidden="true">·</span>
      <span>{formatDuration(sound.durationMs, i18n.language)}</span>
      {sound.status !== "available" && (
        <span className="inline-flex items-center gap-1 font-medium text-destructive">
          <TriangleAlertIcon aria-hidden="true" className="size-3" />
          {sound.status === "missing" ? t("sound.missing") : t("sound.damaged")}
        </span>
      )}
    </span>
  );
}

/** Shown while a sound is sounding. The text is for assistive technology as well as for the eye. */
export function PlayingBadge() {
  const { t } = useTranslation();
  return (
    <span className="inline-flex items-center gap-1 text-xs font-medium text-primary">
      <AudioLinesIcon aria-hidden="true" className="size-3.5" />
      {t("sound.playing")}
    </span>
  );
}
