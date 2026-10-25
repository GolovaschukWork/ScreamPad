# Bootstrap sequence

This document records the original bootstrap sequence and the current application setup. The repository now contains a compilable Tauri application; use `package.json` and CI for current commands.

1. Choose product identity, repository slug and bundle identifier with owner.
2. Initialize Tauri 2 React + TS with official scaffolding, pin runtime versions and lockfile.
3. Reconcile generated commands/scripts with CI instead of inventing package scripts.
4. Implement typed IPC spike and first library import/playback vertical slice.
5. Implement virtual-device enumeration and microphone+clip mixing proof of concept.
6. Set up real OS smoke test matrix and signing credentials before public beta.
7. Activate CI gated jobs as scripts/modules become available.

The committed CI workflow checks governance files today. The platform build template should be enabled after application scaffold exists.

## Pinned toolchain

| Tool | Version | Pinned in |
| --- | --- | --- |
| Node.js 24 LTS | 24.22.1 | `.nvmrc`, `package.json` `engines` |
| pnpm 10 | 10.34.8 | `package.json` `packageManager` (Corepack), `engines` |
| Rust stable | 1.99.1 | `rust-toolchain.toml` (rustfmt, clippy, macOS arm64/x64 and Windows x64 targets) |

Setup on a developer machine: `nvm install` (reads `.nvmrc`), `corepack enable`, and install rustup from the official
installer with its published SHA-256 verified; the toolchain is then fetched from `rust-toolchain.toml` on first use.
Versions are bumped deliberately in a dedicated PR, never by "latest".

Product name: **ScreamPad** (owner-approved). Temporary internal identifiers, kept until publisher and domain are decided: package `soundboard-desktop`, bundle identifier
`dev.local.soundboard`. It must never be used for production signing, the updater, or public releases. `package.json`
is `private` and has no `license` field until the owner selects an SPDX identifier.

CI: `.github/workflows/actionlint.yml` lints workflows with a pinned actionlint binary whose SHA-256 is verified and cached.

## Application scaffold

Generated with `create-tauri-app` 4.7.5 (React + TypeScript template, pnpm) and then reduced: the demo `greet` command, the
`opener` plugin and its permission, the mobile entry point and serde dependencies were removed; a strict CSP was added;
`package.json` versions are exact and the lockfiles (`pnpm-lock.yaml`, `src-tauri/Cargo.lock`) are committed.

| Package | Version |
| --- | --- |
| Vite | 8.3.5 |
| TypeScript | 6.0.3 (7.x is available but not adopted until verified with the Vite/React toolchain) |
| React / React DOM | 19.3.1 |
| `@vitejs/plugin-react` | 6.1.2 |
| Tauri CLI / JS API | 2.12.4 |
| `tauri` / `tauri-build` crates | 2.12.5 / 2.7.4 (resolved in `Cargo.lock`) |

Scripts include `pnpm dev`, `pnpm build`, `pnpm typecheck`, `pnpm test:ui`, `pnpm test:scripts`, and `pnpm tauri <cmd>`.
Vitest is configured for frontend component tests (ADR 0012). There is no frontend lint script in `package.json`; do not
assume ESLint or Prettier is configured. Consult the current Tauri capability files for actual permissions.

## CI cost control

CI runs on pull requests only; merging a green PR does not repeat it on `main`. `.github/scripts/detect-changes.sh` maps changed
files to areas, so documentation, ADR and governance-only PRs skip the TypeScript job, the three-OS native matrix and CodeQL.
Frontend-only edits skip the native matrix; Rust-only edits skip the frontend job. The `ci-gate` job is the single status to
require once branch protection is enabled (skipped jobs pass, failures and cancellations do not). CodeQL refreshes its `main`
baseline when sources change on `main` and weekly. Superseded runs on the same ref are cancelled.

## Typed IPC

Commands are typed end to end with `tauri-specta` (ADR 0004). Register a command in `src-tauri/src/ipc.rs`, define its DTOs in
`src-tauri/src/commands/` with `#[derive(specta::Type)]`, then run `pnpm ipc:export` to regenerate
`src/shared/ipc/bindings.ts`. Never edit that file by hand: CI regenerates it and fails on any diff. 64-bit integers need an
explicit mapping; specta refuses to export them otherwise.

## Library database

The SQLite library is `library.db` in the platform application-data directory (`~/Library/Application Support/<identifier>` on
macOS, `%APPDATA%\<identifier>` on Windows), where `<identifier>` is the bundle identifier. While the identifier is the
temporary `dev.local.soundboard`, development data lives in that directory; changing the identifier later starts a new, empty
library. The schema is created by forward-only migrations in `src-tauri/migrations`; adding one requires owner approval, a
checksum entry in `SHIPPED_CHECKSUMS`, and a frozen fixture `src-tauri/tests/fixtures/sqlite/schema_vN.sql` (see ADR 0005).

The release profile uses Rust's default `panic = "unwind"`.

## Localization

ScreamPad ships in English and Ukrainian (ADR 0008). Text lives in `src/shared/i18n/locales/{en,uk}.json`, is looked up by key, and
errors map `AppError.kind` to `errors.<kind>`. `pnpm i18n:check` verifies catalog parity and `pnpm test:scripts` runs the
dependency-free tests for it; both run in CI.

## Managed library files

Imported audio lives under `library/` in the application-data directory: `staging/` (scratch copies, cleared at startup) and
`media/<aa>/<sha256>.<ext>` (read-only managed files). Both directories are on the same volume so the final move is an atomic
rename. See ADR 0007 for the import order, durability and recovery rules. Test audio fixtures are generated by
`src-tauri/tests/fixtures/audio/generate.sh` (needs `ffmpeg`; the generated files are committed).

## Audio core

`src-tauri/src/audio` holds the real-time mixer and the clip type; it knows nothing about devices, files or Tauri, so it is tested
without hardware. `tests/audio_realtime.rs` asserts that `Mixer::render` never allocates or frees memory (ADR 0006). Clips are
decoded and resampled to the device format outside the callback by `infrastructure::media::decode`.

The device engine (`audio/engine.rs`, `infrastructure/audio/cpal_backend.rs`) keeps the output stream alive across device changes.
`cargo test --test audio_device -- --ignored` runs it silently against the real default output on a machine with audio hardware;
`docs/engineering/audio-manual-checklist.md` lists the manual checks CI cannot do.

## Playing sounds

`PlaybackService` plays library sounds through the audio engine: it verifies the managed file against its recorded SHA-256, decodes
it for the current output format, caches the result, and hands it to the engine. The UI lists sounds with a Play button and shows
the state of the audio output. Run `cargo test --test audio_device -- --ignored` on a machine with audio hardware for a silent
end-to-end check of the real output; see `docs/engineering/audio-manual-checklist.md` for the manual device checks.
