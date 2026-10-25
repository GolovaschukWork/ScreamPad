import { useTranslation } from "react-i18next";
import type { SoundSummary } from "../../shared/ipc/bindings";
import type { ViewMode } from "./libraryView";
import { SoundListItem } from "./SoundListItem";
import { SoundTile } from "./SoundTile";

type Props = {
  mode: ViewMode;
  sounds: readonly SoundSummary[];
  playing: ReadonlySet<number>;
  selectedId: number | null;
  onSelect: (id: number) => void;
  onPlay: (id: number) => void;
  onStop: (id: number) => void;
};

/** The sounds as a grid of tiles or a dense list. Both modes get the same data and the same handlers, so there is exactly
 *  one place that knows what is playing or selected. */
export function SoundCollection({ mode, sounds, playing, selectedId, onSelect, onPlay, onStop }: Props) {
  const { t } = useTranslation();
  const Item = mode === "grid" ? SoundTile : SoundListItem;
  return (
    // `role="list"` keeps the list semantics that some screen readers drop from unstyled lists.
    <ul
      role="list"
      aria-label={t("sound.list")}
      data-view={mode}
      className={
        mode === "grid"
          ? "grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-3"
          : "flex flex-col gap-0.5"
      }
    >
      {sounds.map((sound) => (
        <Item
          key={sound.id}
          sound={sound}
          playing={playing.has(sound.id)}
          selected={sound.id === selectedId}
          onSelect={onSelect}
          onPlay={onPlay}
          onStop={onStop}
        />
      ))}
    </ul>
  );
}
