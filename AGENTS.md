# ScreamPad engineering entrypoint for Codex

ScreamPad is a local-first desktop soundboard for Windows 11 x64 and macOS 14+ arm64/x64. Stack: Tauri 2, React, TypeScript, Rust, SQLite. Preserve the feature-based React frontend and modular-monolith Rust backend. Rust owns audio, storage, media, and devices; IPC uses generated types.

Read the [architecture](docs/architecture/overview.md), [Accepted ADRs](docs/architecture/decisions/README.md), [engineering standards](docs/engineering/standards.md), [development workflow](docs/engineering/development-workflow.md), [Definition of Done](docs/engineering/definition-of-done.md), [Git policy](docs/engineering/git-policy.md), and [release policy](docs/engineering/release-policy.md) as relevant. These are the source of truth. Keep changes focused, preserve Accepted ADRs and shipped migrations, verify work, and mark untested platform behavior **NOT VERIFIED**.

Seek owner approval at the boundaries in the shared workflow. Never expose secrets, force-push, merge into main, or publish a release without explicit approval. Use Conventional Commits and draft PRs for coherent, approved work when GitHub access is available.

Communicate with the owner in Ukrainian. Write repository files, comments, commits, and PR descriptions in English. Repo skills live in `.agents/skills/`.
