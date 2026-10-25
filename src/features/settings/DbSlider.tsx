import { useEffect, useId, useRef, useState } from "react";
import { dbToMillibels, formatDb, millibelsToDb } from "../../shared/format/gain";
import { Slider } from "@/shared/ui/slider";

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
  // A value the user chose that has not been handed to `onCommit` yet.
  const pending = useRef<number | null>(null);
  const latest = useRef({ valueMillibels, onCommit });
  latest.current = { valueMillibels, onCommit };

  // Follow the stored value when it changes from outside (for example after a save).
  useEffect(() => setDraft(valueMillibels), [valueMillibels]);

  const flush = () => {
    window.clearTimeout(timer.current);
    const value = pending.current;
    pending.current = null;
    if (value !== null && value !== latest.current.valueMillibels) latest.current.onCommit(value);
  };
  // Leaving the page within the delay must not lose the last change.
  useEffect(() => flush, []);

  return (
    <div className="flex w-72 max-w-full items-center gap-3">
      <span id={id} className="sr-only">
        {label}
      </span>
      <Slider
        className="min-w-0 flex-1"
        aria-label={label}
        min={millibelsToDb(minMillibels)}
        max={millibelsToDb(maxMillibels)}
        step={1}
        value={[millibelsToDb(draft)]}
        getAriaValueText={(_formatted, value) => formatDb(dbToMillibels(value), locale)}
        onValueChange={(value) => {
          const db = Array.isArray(value) ? value[0] : value;
          if (db === undefined) return;
          const next = dbToMillibels(db);
          setDraft(next);
          pending.current = next;
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(flush, COMMIT_DELAY_MS);
        }}
      />
      <output htmlFor={id} className="w-14 shrink-0 text-right text-sm whitespace-nowrap tabular-nums text-muted-foreground">
        {formatDb(draft, locale)}
      </output>
    </div>
  );
}
