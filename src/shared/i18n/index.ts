import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import uk from "./locales/uk.json";
import { DEFAULT_LANGUAGE, resolveLanguage } from "./resolveLanguage";

// Typed keys: an unknown key is a compile error. Interpolation parameter names are checked by `pnpm i18n:check`,
// not by the compiler (ADR 0008).
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
    resources: { translation: typeof en };
  }
}

const resources = { en: { translation: en }, uk: { translation: uk } };

/** Starts localization synchronously from the system language, with English as the fallback. */
export function initI18n(preferred: readonly string[] = navigator.languages): void {
  const language = resolveLanguage(preferred);
  void i18next.use(initReactI18next).init({
    lng: language,
    fallbackLng: DEFAULT_LANGUAGE,
    resources,
    interpolation: { escapeValue: false },
    initAsync: false,
  });
  document.documentElement.lang = language;
  i18next.on("languageChanged", (next) => {
    document.documentElement.lang = next;
  });
}

/** Switches the interface to a stored language setting: `system` follows the system language, a tag selects that language. */
export async function applyLanguage(setting: string, system: readonly string[] = navigator.languages): Promise<void> {
  const language = setting === "system" ? resolveLanguage(system) : resolveLanguage([setting]);
  await i18next.changeLanguage(language);
}
