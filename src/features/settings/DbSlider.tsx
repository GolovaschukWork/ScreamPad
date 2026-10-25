import { useEffect, useId, useRef, useState } from "react";
import { dbToMillibels, formatDb, millibelsToDb } from "../../shared/format/gain";

const COMMIT_DELAY_MS = 300;

type Props = {
  label: string;
  /** The stored value, in millibels. */
  valueMillibels: number;
  minMillibels: number;
  maxMillibels: number;
  locale: string;
  /** Called once the user has stopped moving the slider. */
  onCommit: (millibels: number) => void;
};

/** A whole-decibel slider that shows the value and saves it shortly after the last change. */
export function DbSlider({ label, valueMillibels, minMillibels, maxMillibels, locale, onCommit }: Props) {
  const id = useId();
  const [draft, setDraft] = useState(valueMillibels);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef({ valueMillibels, onCommit });
  latest.current = { valueMillibels, onCommit };

  // Follow the stored value when it changes from outside (for example after a save).
  useEffect(() => setDraft(valueMillibels), [valueMillibels]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const commitNow = (millibels: number) => {
    window.clearTimeout(timer.current);
    if (millibels !== latest.current.valueMillibels) latest.current.onCommit(millibels);
  };

  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        type="range"
        min={millibelsToDb(minMillibels)}
        max={millibelsToDb(maxMillibels)}
        step={1}
        value={millibelsToDb(draft)}
        aria-valuetext={formatDb(draft, locale)}
        onChange={(event) => {
          const next = dbToMillibels(Number(event.currentTarget.value));
          setDraft(next);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => commitNow(next), COMMIT_DELAY_MS);
        }}
        onBlur={() => commitNow(draft)}
      />
      <output htmlFor={id}>{formatDb(draft, locale)}</output>
    </div>
  );
}
