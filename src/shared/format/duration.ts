/** Formats a duration in milliseconds for display: seconds with one decimal under a minute, `m:ss` from a minute on. */
export function formatDuration(milliseconds: number, locale: string): string {
  const total = Math.max(0, Math.round(milliseconds));
  if (total < 60_000) {
    return new Intl.NumberFormat(locale, {
      style: "unit",
      unit: "second",
      unitDisplay: "narrow",
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    }).format(total / 1000);
  }
  const seconds = Math.round(total / 1000);
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}
