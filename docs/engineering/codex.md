# Codex in ScreamPad

Codex CLI 0.164.0 was used when this setup was written. Start it from the repository root; Codex reads `AGENTS.md`, discovers `.agents/skills/*/SKILL.md`, and loads `.codex/config.toml` for a trusted project. Use `$implement`, `$review`, `$verify`, `$adr`, or `$release` in a prompt. Skills may also be selected by description. Read `development-workflow.md` and `standards.md` for provider-independent policy.

Three optional project custom agents live in `.codex/agents/`: Architect reviews architecture and ADRs, Reviewer reviews code and tests, and Verifier runs scoped checks. Use them only when focused independent work helps. Architect and Reviewer have the shell, apps, plugins, memory, and permission-request tools disabled. Verifier retains the shell for checks but is instructed not to edit. Custom agents inherit the parent sandbox and approval policy; their text instructions are not a separate read-only filesystem sandbox.

`.codex/config.toml` requests `workspace-write`, `on-request` approval, and no sandboxed command network access. These are client sandbox settings, subject to trust, higher-priority configuration, host behavior, and CLI overrides. The repo does not itself enforce a denylist for secret reads, Git commands, or release publication. Owner-approval boundaries in `AGENTS.md` and shared policy are instructions; GitHub branch protection, credential scoping, repository permissions, and release protection must enforce server-side restrictions. Never use bypass modes for this project.

Claude Code's `.claude/settings.json` and hooks are separate and are not automatically used by Codex. The same engineering standards apply to both tools. A successful local check does not establish Windows/macOS hardware or production signing behavior.
