import type { SoundSummary } from "../../shared/ipc/bindings";
import { cn } from "@/shared/lib/utils";
import { SoundActions } from "./SoundActions";
import { PlayingBadge, SoundMeta } from "./SoundMeta";

type Props = {
  sound: SoundSummary;
  playing: boolean;
  selected: boolean;
  onSelect: (id: number) => void;
  onPlay: (id: number) => void;
  onStop: (id: number) => void;
};

/** One sound in the grid. */
export function SoundTile({ sound, playing, selected, onSelect, onPlay, onStop }: Props) {
  return (
    <li
      data-playing={playing ? "true" : undefined}
      className={cn(
        "flex min-w-0 flex-col gap-3 rounded-lg border bg-card p-3 text-card-foreground transition-colors",
        // Off-screen tiles are skipped by the browser's layout and paint, which keeps large libraries responsive.
        "[contain-intrinsic-size:auto_7rem] [content-visibility:auto]",
        selected && "bg-accent",
        playing && "border-primary",
      )}
    >
      <button
        type="button"
        aria-pressed={selected}
        onClick={() => onSelect(sound.id)}
        className="flex min-w-0 flex-col gap-1 rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <span className="block truncate text-sm font-medium" title={sound.displayName}>
          {sound.displayName}
        </span>
        <SoundMeta sound={sound} />
      </button>
      <div className="flex items-center justify-between gap-2">
        <SoundActions sound={sound} playing={playing} onPlay={onPlay} onStop={onStop} />
        {playing && <PlayingBadge />}
      </div>
    </li>
  );
}
