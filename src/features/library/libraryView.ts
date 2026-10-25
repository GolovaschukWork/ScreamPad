import type { SoundSummary } from "../../shared/ipc/bindings";

export type ViewMode = "grid" | "list";
export type SortKey = "name" | "duration" | "recent";

export const VIEW_MODES: readonly ViewMode[] = ["grid", "list"];
export const SORT_KEYS: readonly SortKey[] = ["name", "duration", "recent"];

/** Sounds whose name contains the query, ignoring case in the active language ("Гудок" is found by "гуд"). */
export function filterSounds(sounds: readonly SoundSummary[], query: string, locale: string): SoundSummary[] {
  const needle = query.trim().toLocaleLowerCase(locale);
  if (needle === "") return [...sounds];
  return sounds.filter((sound) => sound.displayName.toLocaleLowerCase(locale).includes(needle));
}

/** A sorted copy. Names sort in the active language with numbers in numeric order ("Sound 2" before "Sound 10");
 *  `recent` puts the sounds added last first (ids grow with every import). */
export function sortSounds(sounds: readonly SoundSummary[], key: SortKey, locale: string): SoundSummary[] {
  const collator = new Intl.Collator(locale, { sensitivity: "base", numeric: true });
  const byName = (a: SoundSummary, b: SoundSummary) => collator.compare(a.displayName, b.displayName) || a.id - b.id;
  const copy = [...sounds];
  switch (key) {
    case "name":
      return copy.sort(byName);
    case "duration":
      return copy.sort((a, b) => a.durationMs - b.durationMs || byName(a, b));
    case "recent":
      return copy.sort((a, b) => b.id - a.id);
  }
}
