# ADR 0011: UI Stack (Tailwind CSS v4, shadcn/ui on Base UI, Lucide, Sonner)

- Status: Accepted (owner approved on 2026-10-25 with the decisions below)
- Date: 2026-10-25
- Owner: project owner

## Context
The current UI is plain markup with a small hand-written stylesheet (PR #30 adds tokens and system light/dark). The product needs
a premium, dark-first desktop look with Dark, Light and System themes, accessible keyboard and focus behaviour, and reusable
components for the first vertical slice. Existing stack: React 19.3.1, TypeScript 6.0.3, Vite 8.3.5, Tauri 2 (WKWebView on macOS,
WebView2 on Windows), i18next (ADR 0008), generated IPC bindings (ADR 0004).

## Decision (proposed)
- **Tailwind CSS v4** with the first-party Vite plugin, CSS-first configuration (`@theme`), no `tailwind.config.js`.
- **shadcn/ui, Base UI variant.** Components are copied into the repository (owned code), added one by one as the UI needs them,
  never the whole library. One primitive library only (Base UI); Radix is not mixed in.
- **Lucide React** for icons, **Sonner** for notifications.
- **Motion** is not installed now. It needs its own approval with a concrete need, and never runs on audio-critical paths.
- **Design direction:** premium desktop, dark-first, original branding (no Discord/Spotify assets, names or layouts copied).
- **Themes:** Dark, Light and System. System follows `prefers-color-scheme`.
- **Theme persistence:** the only persistent source of truth is SQLite `app_settings`, through a forward-only migration 0003:
  `theme TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'dark', 'light'))`. It ships in its own PR, separate from the
  UI foundation, with the standard schema tests, a new frozen fixture and a `SHIPPED_CHECKSUMS` entry. `localStorage` is not
  used as a second store for the theme.
- **Startup:** the system theme applies first, with CSS only (`prefers-color-scheme`, no script), so there is no unstyled or
  wrong-scheme frame before JavaScript runs. The persisted theme is read before the first React render (as the language already
  is) and applied as a class on `<html>`; a failed read keeps System. A brief frame in the system scheme is possible while that
  one IPC call runs when the stored theme differs from the system one (NOT VERIFIED on a real window).
- **Single source of truth for design values:** colors, typography, spacing and radius are semantic CSS variables in one
  stylesheet, mapped to Tailwind through `@theme`. Feature components never contain raw colors.
- **Tokens:** semantic CSS variables (`--background`, `--foreground`, `--primary`, `--muted`, `--border`, `--ring`, ...) defined in
  one stylesheet for light and dark. Feature components use semantic utilities only, with no hardcoded colors.
- **Layout of code:** shared shadcn components in `src/shared/ui`, helpers in `src/shared/lib`, feature code in `src/features/*`.
  `shared/` never imports from `features/`. `SoundCard`, `Waveform` and `AudioControls` are custom feature components built on
  these primitives, in separate changes.
- **Accessibility:** semantic controls, keyboard navigation, visible focus rings, `prefers-reduced-motion`, contrast checked
  for both themes. No animation in real-time audio paths; the UI never performs DSP (CLAUDE.md).

## Options
- **A: Tailwind v4 + shadcn (Base UI) + Lucide + Sonner** (proposed). Owned component source, strong accessibility primitives,
  tokens as CSS variables. Cost: more dependencies and a build step for CSS.
- **B: Keep hand-written CSS (PR #30) and add headless primitives only when needed.** Fewer dependencies, but every control is
  built and tested by us.
- **C: A full component kit (MUI, Mantine, Chakra).** Faster start, harder to give the app its own look, heavier runtime.

## Compatibility check (executed on 2026-10-25, from package metadata and the official Vite guide; nothing installed yet)
- `@tailwindcss/vite 4.3.3` declares peer `vite ^5.2 || ^6 || ^7 || ^8`, so it accepts Vite 8.3.5.
- `@base-ui/react 1.9.0`, `sonner 2.0.8`, `lucide-react 1.61.0` accept React 19.
- shadcn's Vite guide needs a `@/*` path alias in `tsconfig.json` and `vite.config.ts` and a `components.json`; its aliases can
  point at `src/shared/ui` and `src/shared/lib`. The CLI offers Base UI at `init` (`--base base-ui`); the exact flags are to be
  confirmed when the foundation PR runs it.
- Tailwind v4 needs a modern engine (Chrome 111, Safari 16.4, Firefox 128). WKWebView on macOS 14+ and the evergreen WebView2 on
  Windows 11 meet this. NOT VERIFIED inside the real Tauri windows.
- The CLI downloads and writes source files. It is run as a pinned one-off (`pnpm dlx shadcn@4.22.1 ...`), and every generated
  file is reviewed in the diff before commit. The `shadcn` package is not added to the project unless a real need appears, and that
  would need a new approval.

## Dependencies requested for approval (exact pins; all licenses permissive; release dates from the registry)
| Package | Version | Kind | License | Released |
| --- | --- | --- | --- | --- |
| `tailwindcss` | 4.3.3 | dev | MIT | 2026-07-16 |
| `@tailwindcss/vite` | 4.3.3 | dev | MIT | (same release line) |
| `@base-ui/react` | 1.9.0 | runtime | MIT | 2026-10-09 |
| `lucide-react` | 1.61.0 | runtime | ISC | 2026-10-21 |
| `sonner` | 2.0.8 | runtime | MIT | 2026-08-09 |
| `class-variance-authority` | 0.7.1 | runtime | Apache-2.0 | |
| `clsx` | 2.1.1 | runtime | MIT | |
| `tailwind-merge` | 3.7.0 | runtime | MIT | |
| `tw-animate-css` | 1.4.0 | dev (CSS only), only if a generated component needs it | MIT | |
| `@types/node` | 24.19.2 | dev | MIT | |
| `shadcn` | 4.22.1 | **not added to the project**; used only as a pinned one-off `pnpm dlx shadcn@4.22.1` to generate source | MIT | 2026-10-18 |

Not requested: `motion` (14.6.0, MIT) and any font package. Fonts stay the system stack. Versions are re-verified at install time
and installs use the lockfile with `--frozen-lockfile` in CI. License identifier of ScreamPad is still TBD; Apache-2.0 and ISC
are compatible with GPL-3.0 and with permissive choices, to be confirmed in the pending license audit.

## Design decisions carried over from the hand-written styling (PR #30, not merged)
The useful parts of #30 move into the foundation, expressed as semantic tokens: system light/dark through
`prefers-color-scheme`; a visible 3 px focus ring with offset; a 2.5 rem minimum control height; reduced motion disables
transitions; AA-oriented contrast pairs for text, muted text, accent and danger; layout rules for narrow windows; sound rows
as bordered cards. #30 is closed unmerged once this ADR is accepted, because its plain CSS conflicts with Tailwind's preflight.

## Implementation plan
Foundation PR (no SQLite change, no Rust, no audio, no Motion, no font packages, existing screens not rewritten):
1. Tailwind Vite plugin; TypeScript and Vite `@/` aliases; `components.json` pointing at `src/shared/ui` and `src/shared/lib`.
2. One stylesheet with semantic tokens (dark-first, light, system) mapped to Tailwind.
3. `ThemeProvider` (system, dark, light) with tested pure resolution logic. Until migration 0003 lands it holds the value in
   memory only; it does not persist anything.
4. `Button`, `Select`, `Slider`, `Switch` and `Sonner` only.
5. Theme tests and Dark/Light screenshots; WebView checks on macOS and Windows through CI as far as CI allows.

Migration PR (separate, after the foundation): migration 0003 (`theme`), `get_settings`/`update_settings` carry the theme,
the provider reads it before the first render.

## Consequences
- Components are our code, so upgrades are manual diffs, not version bumps.
- More dependencies to audit (supply chain: pinned versions, lockfile, reviewed generated source).
- Two styling systems briefly coexist (hand CSS and Tailwind) until the existing views are migrated.

## Decisions on the open questions (owner, 2026-10-25)
1. Theme storage: SQLite `app_settings` only; migration 0003 allowed as described above; no `localStorage`.
2. The dependency table is approved (shadcn only as a pinned one-off, `tw-animate-css` only if needed).
3. PR #30 is not merged; its useful decisions are recorded here and it is closed with an explanation.
4. Base UI, not Radix.

## Implementation notes (UI foundation, 2026-10-25)
- Installed with exact pins as approved (`pnpm audit --prod`: no known vulnerabilities). `shadcn` is not a dependency: the five
  components were generated once with `pnpm dlx shadcn@4.22.1 add ...` in a scratch copy and reviewed before being copied in.
- Corrections made to the generated source, which a plain `shadcn add` without `init` gets wrong in this layout:
  - it imported `cn` from an unrelated npm package named `cn` (and added it); replaced with our `@/shared/lib/utils`;
  - `sonner.tsx` used `next-themes` (not approved); it now uses our own `useTheme`;
  - the slider thumb used `bg-white`; it uses `bg-background`;
  - `data-horizontal:` / `data-vertical:` never matched because Base UI emits `data-orientation`; two `@custom-variant`
    lines in `index.css` map them (without them the slider track collapsed);
  - Next.js-only `"use client"` and a `cn-toast` class (defined by an `init` stylesheet we do not use) were removed.
- Tokens live only in `src/app/index.css` (`:root` light, `:root.dark`, `@theme inline`), seeded from the useful decisions of the
  closed PR #30. `scripts/theme/tokens.test.mjs` checks that both themes define the same set, AA contrast (4.5:1) for text
  pairs, 3:1 for the focus ring, and that no raw color appears outside that file.
- `ThemeProvider` holds the choice in memory only, always puts exactly one of `dark`/`light` on `<html>`, follows the system
  scheme live, and persists nothing. The pre-JavaScript frame follows the system scheme with CSS alone. Persistence arrives with
  migration 0003 in its own PR.
- Tailwind's preflight now resets browser defaults, so the existing, unrewritten screens look plainer (unstyled buttons, no list
  markers). They stay functional; migrating them is a separate approved change.
- Verified: typecheck, build, 23 script tests (with mutation checks), and a temporary gallery in Chrome (Dark, Light, System,
  live switching, Select popup, slider, switch, keyboard focus ring). NOT VERIFIED: the real Tauri WebViews (WKWebView, WebView2).
  CI builds the frontend and the native targets, but does not render the WebView, so no CI screenshot exists.

## Implementation notes (theme persistence, 2026-10-25)
- Migration `0003_theme_setting.sql` adds `app_settings.theme` (`system|dark|light`, default `system`, CHECK). Frozen fixture
  `schema_v3.sql`, `SHIPPED_CHECKSUMS` entry, upgrade tests 1 -> 3 and 2 -> 3 with the pre-upgrade backup.
- `Theme` is a domain enum; `ThemeDto` is the typed IPC value. `get_settings` returns it and `update_settings` takes it; an
  unknown value is refused by deserialization and never reaches the service. A stored value that does not parse is
  `storageCorrupt`, like the other settings.
- Startup: `main.tsx` reads the settings once before the first render, applies the language and passes the theme to
  `ThemeProvider`. Until then nothing is rendered and CSS already follows the system scheme. If the read fails, `system` stays.
- Saving a setting updates the database first; the provider then mirrors the stored value. `localStorage` is not used.
- A native `<select>` was added to the existing settings panel for the theme (the panel itself is not restyled).
- NOT VERIFIED: the first frame in the real Tauri windows when the stored theme differs from the system one.
