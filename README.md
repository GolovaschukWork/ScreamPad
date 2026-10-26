# ScreamPad

ScreamPad is a local-first desktop soundboard for Windows 11 x64 and macOS 14+ (arm64/x64). It uses Tauri 2, React, TypeScript, Rust, and SQLite. The product is under development; platform audio behavior and production distribution remain subject to the verification and release gates in `docs/engineering/`.

Start with [CONTRIBUTING.md](CONTRIBUTING.md), [architecture](docs/architecture/overview.md), and the [Accepted ADRs](docs/architecture/decisions/README.md). ScreamPad is licensed under [GPL-3.0-only](LICENSE). Publisher identity, domain, and the production bundle identifier remain undecided. Do not publicly distribute a release before the dependency/license audit and the other release gates are complete.

## AI development

- Codex: `AGENTS.md`, `.agents/skills/`, and `.codex/`.
- Claude Code: `CLAUDE.md` and `.claude/`.
- Shared source of truth: `docs/engineering/standards.md`, `development-workflow.md`, `definition-of-done.md`, `git-policy.md`, and `release-policy.md`.

Start Codex from the repository root so it discovers `AGENTS.md` and repo skills. Use `$implement`, `$review`, `$verify`, `$adr`, or `$release` in a prompt as appropriate. The Architect, Reviewer, and Verifier custom roles are available to Codex for focused work; routine tasks need no delegation. See [Codex setup and permission limits](docs/engineering/codex.md).

Claude hooks do not run in Codex. Repository instructions describe required conduct; the Codex sandbox and GitHub branch protection provide separate technical controls.
