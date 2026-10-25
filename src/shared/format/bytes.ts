/** A file size for display, for example `1.4 MB`, in the locale's number format. Sizes use powers of 1024. */
export function formatBytes(bytes: number, locale: string): string {
  const value = Math.max(0, Number.isFinite(bytes) ? bytes : 0);
  const units = ["byte", "kilobyte", "megabyte", "gigabyte"] as const;
  let index = 0;
  let scaled = value;
  while (scaled >= 1024 && index < units.length - 1) {
    scaled /= 1024;
    index += 1;
  }
  return new Intl.NumberFormat(locale, {
    style: "unit",
    unit: units[index],
    unitDisplay: "short",
    maximumFractionDigits: index === 0 ? 0 : 1,
  }).format(scaled);
}
