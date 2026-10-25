# Bootstrap sequence

This repository kit intentionally contains **governance and project scaffolding only**, not a compilable Tauri application.

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

Scripts: `pnpm dev`, `pnpm build`, `pnpm typecheck`, `pnpm tauri <cmd>`. Lint, format and unit-test tooling (ESLint,
Prettier, Vitest) is intentionally absent until its dependencies are approved. The capability set is `core:default` only.
