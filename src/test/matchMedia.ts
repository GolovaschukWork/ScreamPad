type Listener = () => void;

/** A controllable stand-in for `window.matchMedia`. Tests flip `matches` for a query and the listeners are told. */
const state = new Map<string, boolean>();
const listeners = new Map<string, Set<Listener>>();

export function installMatchMedia(initial: Record<string, boolean> = {}): void {
  state.clear();
  listeners.clear();
  for (const [query, matches] of Object.entries(initial)) state.set(query, matches);
  window.matchMedia = (query: string): MediaQueryList => {
    const set = listeners.get(query) ?? new Set<Listener>();
    listeners.set(query, set);
    return {
      get matches() {
        return state.get(query) ?? false;
      },
      media: query,
      onchange: null,
      addEventListener: (_type: string, listener: EventListenerOrEventListenerObject) => {
        if (typeof listener === "function") set.add(listener as Listener);
      },
      removeEventListener: (_type: string, listener: EventListenerOrEventListenerObject) => {
        set.delete(listener as Listener);
      },
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    };
  };
}

/** Changes whether a media query matches and notifies the components listening to it. */
export function setMediaMatches(query: string, matches: boolean): void {
  state.set(query, matches);
  for (const listener of listeners.get(query) ?? []) listener();
}
