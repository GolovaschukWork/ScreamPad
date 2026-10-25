export const SUPPORTED_LANGUAGES = ["en", "uk"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];
export const DEFAULT_LANGUAGE: Language = "en";

function isSupported(code: string): code is Language {
  return (SUPPORTED_LANGUAGES as readonly string[]).includes(code);
}

/** Picks the first preferred language (for example `navigator.languages`) whose primary subtag is supported. */
export function resolveLanguage(preferred: readonly string[]): Language {
  for (const tag of preferred) {
    const primary = tag.trim().toLowerCase().split(/[-_]/)[0] ?? "";
    if (isSupported(primary)) return primary;
  }
  return DEFAULT_LANGUAGE;
}
