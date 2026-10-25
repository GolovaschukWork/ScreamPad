# ScreamPad engineering entrypoint for Claude Code

> Product name: ScreamPad (owner-approved). License: TBD. Publisher, domain, and production bundle identifier remain undecided.

ScreamPad is a local-first desktop soundboard for Windows 11 x64 and macOS 14+ arm64/x64. The stack is Tauri 2, React, TypeScript, Rust, and SQLite. Read the [shared standards](docs/engineering/standards.md), [development workflow](docs/engineering/development-workflow.md), [architecture](docs/architecture/overview.md), and [Accepted ADRs](docs/architecture/decisions/README.md). Those documents are the source of truth.

Claude-specific rules, skills, agents, and Bash guard live in `.claude/`. They do not apply to Codex. Follow shared approval and verification requirements even if a tool can act without prompting.

Communicate with the owner in Ukrainian. Repository files, code, comments, commit messages, and PR descriptions are in English.
