import type { TFunction } from "i18next";
import type en from "../i18n/locales/en.json";
import type { AppError } from "./bindings";

type CatalogErrorKeys = keyof typeof en.errors;
type KindsWithoutText = Exclude<AppError["kind"], CatalogErrorKeys>;
type Assert<T extends true> = T;

/** Fails to compile when a backend error kind has no text in the catalog. */
export type EveryErrorKindHasText = Assert<[KindsWithoutText] extends [never] ? true : false>;

/** Text for a typed backend error, in the active language. Parameters come from the error itself. */
export function errorText(t: TFunction, error: AppError): string {
  switch (error.kind) {
    case "storageTooNew":
      return t("errors.storageTooNew", { database: error.database, supported: error.supported });
    default:
      return t(`errors.${error.kind}`);
  }
}
