import { useSyncExternalStore } from "react";
import type { SortKey, ViewMode } from "./libraryView";

/** How the library is presented. It lives for the session only (it is not stored in the database), and outside the page
 *  component so that leaving for Settings and coming back keeps the choice. */
export type LibraryViewState = { mode: ViewMode; sort: SortKey; query: string };

const INITIAL: LibraryViewState = { mode: "grid", sort: "name", query: "" };

let state: LibraryViewState = INITIAL;
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useLibraryViewState(): readonly [LibraryViewState, (patch: Partial<LibraryViewState>) => void] {
  const current = useSyncExternalStore(subscribe, () => state);
  return [
    current,
    (patch) => {
      state = { ...state, ...patch };
      for (const listener of listeners) listener();
    },
  ];
}

/** Back to the defaults. Used by tests so one test does not leak its view into the next. */
export function resetLibraryViewState(): void {
  state = INITIAL;
  for (const listener of listeners) listener();
}
