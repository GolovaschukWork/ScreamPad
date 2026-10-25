import { useEffect, useState } from "react";
import { commands, events } from "../../shared/ipc/bindings";

/** The ids of the sounds that are sounding right now. The backend is the only source: the initial answer comes from
 *  `playing_sounds` and every change arrives as a `PlayingChanged` event (a newer event wins over a slower initial answer).
 *  If the backend cannot say (no audio output), nothing is shown as playing. */
export function usePlayingSounds(): ReadonlySet<number> {
  const [ids, setIds] = useState<ReadonlySet<number>>(new Set());

  useEffect(() => {
    let active = true;
    let sawEvent = false;
    let unlisten: (() => void) | undefined;

    events.playingChanged
      .listen((event) => {
        if (!active) return;
        sawEvent = true;
        setIds(new Set(event.payload.ids));
      })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch(() => {});

    commands
      .playingSounds()
      .then((result) => {
        if (active && !sawEvent && result.status === "ok") setIds(new Set(result.data));
      })
      .catch(() => {});

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  return ids;
}
