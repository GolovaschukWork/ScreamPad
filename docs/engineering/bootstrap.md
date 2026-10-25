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
