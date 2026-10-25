import { PlayIcon, SquareIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SoundSummary } from "../../shared/ipc/bindings";
import { Button } from "@/shared/ui/button";

/** Play and stop for one sound. Playing triggers the sound with its own play mode; stop ends every voice of it. Used by the
 *  grid and the list alike, so both behave the same. */
export function SoundActions({
  sound,
  playing,
  onPlay,
  onStop,
  reserveStopSpace = false,
}: {
  sound: SoundSummary;
  playing: boolean;
  onPlay: (id: number) => void;
  onStop: (id: number) => void;
  /** Keep the room of the stop button while it is hidden, so names in a column stay aligned. */
  reserveStopSpace?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-1">
      <Button
        type="button"
        size="icon"
        disabled={sound.status !== "available"}
        aria-label={t("sound.play", { name: sound.displayName })}
        onClick={() => onPlay(sound.id)}
      >
        <PlayIcon aria-hidden="true" />
      </Button>
      {!playing && reserveStopSpace && <span aria-hidden="true" className="size-8 shrink-0" />}
      {playing && (
        <Button
          type="button"
          size="icon"
          variant="secondary"
          aria-label={t("sound.stop", { name: sound.displayName })}
          onClick={() => onStop(sound.id)}
        >
          <SquareIcon aria-hidden="true" />
        </Button>
      )}
    </div>
  );
}
