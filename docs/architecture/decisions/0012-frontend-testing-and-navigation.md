# ADR 0012: Frontend Component Tests and In-App Navigation

- Status: Accepted (owner approved the test stack and the plan on 2026-10-25; exact versions below were verified before install)
- Date: 2026-10-25
- Owner: project owner

## Context
The application shell, navigation, settings and library screens need tests of behaviour that pure-function tests cannot reach:
navigation, view switching, loading/empty/error states, keyboard use, theme switching and settings persistence. The repository
has only `node:test` checks of pure `.ts` modules (`pnpm test:scripts`); there is no DOM test runner, ESLint or Prettier.
The application has no URLs: it is a single desktop window with a few top-level pages.

## Decision
- **Component tests with Vitest on jsdom**, React Testing Library and user-event. Tests live next to the code as
  `*.test.ts(x)`, import Vitest APIs explicitly (no globals), and replace only the IPC boundary (`commands` from the generated
  bindings) with fakes. Production components never contain test doubles or fake data.
- `pnpm test:ui` runs them; CI runs it in the frontend job. `pnpm test:scripts` stays for the pure script checks.
- **Navigation without a router.** One typed configuration (`src/app/navigation.ts`) lists the pages (id, translation key, icon);
  the shell keeps the current page in React state and renders the matching page. Sections whose feature does not exist yet
  (Boards, Audio Routing) are absent from the configuration, not shown as disabled screens. A router is reconsidered only if
  deep links, history or multiple windows are needed.
- **View preference** (grid or list) is session-only state. It is not stored in SQLite.
- ESLint and Prettier are **not** added here; they need their own decision.

## Dependencies (exact pins, dev only, all MIT)
| Package | Version | Released |
| --- | --- | --- |
| `vitest` | 5.0.5 | 2026-10-15 |
| `jsdom` | 30.1.3 | 2026-10-15 |
| `@testing-library/react` | 16.3.3 | 2026-08-27 |
| `@testing-library/dom` (peer of the former) | 10.4.2 | 2026-09-13 |
| `@testing-library/user-event` | 14.6.8 | 2026-10-13 |

Checked: `vitest` accepts Vite 8 and `@types/node >=24`; Testing Library accepts React 19; `jsdom` needs Node `^24.15`
(the repository pins 24.22.1).

## Consequences
- jsdom is not a browser: layout, focus rings and real WebView behaviour still need visual checks (browser and Tauri).
- jsdom lacks `matchMedia` and `ResizeObserver`; the shared test setup provides minimal stand-ins, nothing more.
- Components stay testable by keeping state and IPC calls in hooks and rendering in presentational components.

## Implementation notes (2026-10-25)
- Shell, navigation and settings landed first; the library redesign followed in a second change.
- The library keeps one data source and one set of handlers for both views: `LibraryPage` owns the data (`useLibrary`), the
  audio actions (`useAudio`) and the playing state (`usePlayingSounds`: `playing_sounds` once, then `PlayingChanged` events),
  and hands the same props to `SoundGrid`-style tiles and list rows through `SoundCollection`. A sound is shown as playing
  only when the backend says so, never because it was clicked.
- View mode, sort key and search text live in a small session store (`libraryViewStore`), outside the page, so they survive a
  visit to Settings. They are not persisted.
- Large libraries: tiles and rows use `content-visibility: auto`; no virtualization library was added. Measure before adding one.
- jsdom limits (documented in the tests): the slider thumb is `visibility: hidden` until Base UI measures it, so it is looked up by
  its label; layout, focus rings and real WebView behaviour need the visual checks.
