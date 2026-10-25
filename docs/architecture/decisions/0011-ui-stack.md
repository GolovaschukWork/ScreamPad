# ADR 0011: UI Stack (Tailwind CSS v4, shadcn/ui on Base UI, Lucide, Sonner)

- Status: Proposed (awaiting owner approval of the stack, the dependency list and the open questions)
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
- **Themes:** Dark, Light and System. System follows `prefers-color-scheme`. Theme is applied as a class on `<html>` before first
  paint, and the choice is stored (see open question 1).
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
  file is reviewed in the diff before commit. If the generated CSS imports `shadcn/tailwind.css`, the `shadcn` package becomes a
  dev dependency at the same pinned version (to be confirmed in the foundation PR).

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
| `tw-animate-css` | 1.4.0 | dev (CSS only) | MIT | |
| `@types/node` | 24.19.2 | dev | MIT | |
| `shadcn` | 4.22.1 | dev, only if the generated CSS needs it | MIT | 2026-10-18 |

Not requested: `motion` (14.6.0, MIT) and any font package. Fonts stay the system stack. Versions are re-verified at install time
and installs use the lockfile with `--frozen-lockfile` in CI. License identifier of ScreamPad is still TBD; Apache-2.0 and ISC
are compatible with GPL-3.0 and with permissive choices, to be confirmed in the pending license audit.

## Implementation plan (after approval, separate PR "UI foundation")
1. Add the approved dependencies, the Tailwind Vite plugin, the `@/` alias, `components.json` (aliases to `src/shared/ui`).
2. One stylesheet with semantic tokens for dark and light, and the Tailwind import. Retire the hand-written tokens of PR #30
   only if the owner agrees (open question 3).
3. `ThemeProvider` (Dark, Light, System), `<html class>` applied before first paint, tested on the pure resolution logic.
4. Add only the primitives the first vertical slice needs, for example `button`, `select`, `slider`, `switch`, `sonner`.
5. A small check that renders every theme (script test of the token files for both themes and a manual screenshot pass).
6. The existing views are not rewritten in this PR. Migrating them is a separate, approved change.
No Rust, audio or schema change in the foundation PR.

## Consequences
- Components are our code, so upgrades are manual diffs, not version bumps.
- More dependencies to audit (supply chain: pinned versions, lockfile, reviewed generated source).
- Two styling systems briefly coexist (hand CSS and Tailwind) until the existing views are migrated.

## Open questions for the owner
1. **Where to store the theme choice.** (a) A new `theme` column in `app_settings` (migration 0003, consistent with ADR 0010 and
   shared with the backend; needs schema approval), or (b) `localStorage` (no schema change, per-webview convenience only).
   Recommendation: (a), as its own small PR, with the foundation PR shipping System only until then.
2. Approve the dependency table above (or strike rows).
3. PR #30 (hand-written tokens): merge as an interim, or close it and let the foundation PR replace it? Recommendation: close it.
4. Base UI variant confirmed (not Radix)?
