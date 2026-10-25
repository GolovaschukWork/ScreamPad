/** Gains travel as integer millibels (1/100 dB). The UI shows and edits whole decibels. */
export function millibelsToDb(millibels: number): number {
  return millibels / 100;
}

export function dbToMillibels(db: number): number {
  return Math.round(db) * 100;
}

/** For example `-20 dB`, `+6 dB`, `0 dB`, with the locale's minus sign and digits. */
export function formatDb(millibels: number, locale: string): string {
  const value = new Intl.NumberFormat(locale, {
    signDisplay: "exceptZero",
    maximumFractionDigits: 1,
  }).format(millibelsToDb(millibels));
  return `${value} dB`;
}
