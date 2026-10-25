# ADR 0008: Localization And Translatable Errors

- Status: Proposed
- Date: 2026-10-25
- Owner: TBD

## Context
The owner requires the application to support **two languages** and wants errors (and all other user-facing text) to be addressed by
key and translatable. The two languages are assumed to be **English and Ukrainian**; this must be confirmed.

State of the code today:
- The IPC error contract already is key-based: `AppError` carries a machine-readable `kind` and numbers, never text or paths
  (ADR 0004 and 0005). The backend produces no user-facing strings.
- The UI still holds English strings inline (`describeError`, `App`).
- Schema 1 (ADR 0005) has an index `sounds_display_name ... COLLATE NOCASE` (see "Finding" below).

Facts checked on 2026-10-25 against the current toolchain (TypeScript 6.0.3, React 19.3.1) in a throw-away project:
- `Intl.PluralRules` reports `one, other` for `en` and `one, few, many, other` for `uk`.
- `i18next` 26.5.10 resolved Ukrainian plurals correctly for 0, 1, 2, 4, 5, 11, 12, 21, 22, 25, 101 and 1.5, fell back to English for a
  key missing in Ukrainian, and its typed resources made an unknown key a **compile error** while accepting valid keys and
  interpolation parameters. `react-i18next` 17.1.8 `useTranslation` type-checked under React 19.

## Options
- **Option A: `i18next` + `react-i18next`.** Both MIT. `i18next` has no runtime dependencies; `react-i18next` adds three small ones.
  Typed keys, plurals through the platform's `Intl.PluralRules`, interpolation, fallback chain and runtime language switching are
  built in, and translators can work on plain JSON with common tooling.
- **Option B: FormatJS `react-intl`** (BSD-3-Clause). Native ICU MessageFormat (select, nested plural), but a heavier dependency tree
  and weaker compile-time key checking without extra code generation.
- **Option C: a small in-house layer** (JSON catalogs, `Intl.PluralRules`, simple interpolation, about 100 lines). No dependency,
  but we would own plural handling, fallback, typing, tooling and every future feature (rich text, context).

## Decision
Not decided; owner approval needed (new dependency). **Recommendation: Option A**, because the typed-key, Ukrainian-plural and
fallback behavior is already demonstrated, and ICU select/gender features are not needed for two languages and this UI. Option C is
acceptable if the owner prefers zero dependencies; the rules below apply to either.

### Rules (independent of the library)
1. **Keys everywhere.** No user-facing string literal in components. Keys are namespaced by area (`library.*`, `errors.*`,
   `import.*`, `common.*`), descriptive, and stable once released.
2. **Errors are keys.** The backend sends `kind` and typed parameters only. The UI shows `errors.<kind>` with those parameters.
   A compile-time check (a type that fails when `AppError["kind"]` has a member missing from the English catalog) replaces the
   hand-written exhaustive `switch`, so a new Rust error variant cannot ship without text. Parameters are numbers or fixed
   identifiers, never free text or paths.
3. **Plurals** use the library's plural rules with the language's own categories (`one`, `few`, `many`, `other` for Ukrainian).
   Counts are never concatenated into sentences.
4. **Numbers, dates, durations and sorting** use `Intl` with the active locale: `NumberFormat`, `DateTimeFormat`, `RelativeTimeFormat`
   and `Collator` (for ordered lists).
5. **Catalog parity check** (`pnpm i18n:check`, a dependency-free Node script run in CI): every locale has the same keys, the same
   interpolation placeholders per key, and every plural category its language requires (from `Intl.PluralRules`); no empty values;
   no key unused by the code. English is the fallback and the source of truth.
6. **Selection.** Start from the system language (best match among the supported ones), fall back to English, allow a user override
   without restart, set `<html lang>` and keep `dir="ltr"` (right-to-left is out of scope). The override is **persisted by the
   backend** (a `settings` table in SQLite), which needs its own approved migration; until then the app follows the system language.
7. **Native text.** Tray menu, notifications and other OS-level labels are set from the UI side through commands, so one catalog
   stays the single source of truth. File dialogs use the OS language. The product name is not translated.
8. **User data is not translated** (sound names, tags). Logs, developer messages and repository documentation stay in English.
9. **Layout and accessibility.** Text can grow (Ukrainian is often longer than English): no fixed-width labels, tested with a
   pseudo-locale during development. Accessible names (`aria-label`) come from the catalog; the active language is exposed through `lang`.
10. **Adding a language** is a new catalog file plus its registration; the parity check enforces completeness.

### Catalog format
Plain JSON per locale (`src/shared/i18n/locales/en.json`, `uk.json`) so translators and tools can edit them. The spike used TypeScript
modules; typed keys from JSON imports must be confirmed in the implementation PR (key paths are inferred; values are widened to
`string`).

### Finding: case-insensitive search and ordering of non-ASCII names
SQLite `COLLATE NOCASE` and `lower()` only fold ASCII. Checked with SQLite 3.51: `'Ж' = 'ж' COLLATE NOCASE` is false, `lower('Ж')`
returns `Ж`, and `ORDER BY ... COLLATE NOCASE` puts `арбуз` after `Яблуко`. The index `sounds_display_name` therefore does not make
Cyrillic names sortable or searchable case-insensitively. Proposed handling, with no schema change now: sort and filter the library
list in the application layer with Unicode-aware tools (`Intl.Collator` in the UI for the current list sizes; a Unicode case-folding
crate or ICU collation in Rust if the library grows large). The index can be dropped or replaced in a later, separately approved
migration.

## Consequences
- Every new feature ships keys for both languages, and CI blocks missing translations; this adds a small cost per UI string.
- One new runtime dependency with Option A (two packages), none with Option C.
- A user-facing error never depends on backend wording, so wording can change without touching Rust.
- Persisting the language choice depends on a later migration.
- Cyrillic sorting and search need application-level logic (see the finding).

## Validation
- The parity check script and a compile-time check of `AppError["kind"]` against the catalog (both runnable without a test runner).
- Plural cases for Ukrainian (1, 2, 5, 11, 21, 22, 25, 101, 1.5) and English as automated cases once the UI test tooling is approved.
- A pseudo-locale pass over every screen to catch truncation; keyboard and screen-reader labels checked in both languages.
- Windows 11 and macOS system-language detection (`navigator.language` inside the webview) is NOT VERIFIED until tested on both.
