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

Temporary application identity for the internal scaffold only: package `soundboard-desktop`, bundle identifier
`dev.local.soundboard`. It must never be used for production signing, the updater, or public releases. `package.json`
is `private` and has no `license` field until the owner selects an SPDX identifier.

CI: `.github/workflows/actionlint.yml` lints workflows with a pinned actionlint binary whose SHA-256 is verified and cached.
