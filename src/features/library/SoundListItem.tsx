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

/** One sound in the list: a dense row. */
export function SoundListItem({ sound, playing, selected, onSelect, onPlay, onStop }: Props) {
  return (
    <li
      data-playing={playing ? "true" : undefined}
      className={cn(
        "flex min-w-0 items-center gap-3 rounded-md border border-transparent px-2 py-1.5 transition-colors hover:bg-accent/60",
        "[contain-intrinsic-size:auto_3rem] [content-visibility:auto]",
        selected && "bg-accent",
        playing && "border-primary",
      )}
    >
      <SoundActions sound={sound} playing={playing} onPlay={onPlay} onStop={onStop} reserveStopSpace />
      <button
        type="button"
        aria-pressed={selected}
        onClick={() => onSelect(sound.id)}
        className="flex min-w-0 flex-1 flex-col rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <span className="block truncate text-sm font-medium" title={sound.displayName}>
          {sound.displayName}
        </span>
        <SoundMeta sound={sound} />
      </button>
      {playing && <PlayingBadge />}
    </li>
  );
}
