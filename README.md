# Desktop soundboard — project governance starter

Product name: **ScreamPad** · License: **TBD**

This is the agreed **engineering governance kit**, not an application implementation. It contains Claude Code instructions, rules, agents, skills, permissions/hooks, architecture docs, ADRs, Git and release policies, templates, and bootstrap validation CI.

See `docs/engineering/bootstrap.md` for application initialization. No license is granted by this repository until a license is selected; clarify before accepting outside contributions or publicly distributing the project.

Product: premium local-first soundboard for Windows 11 x64 and macOS 14+ arm64/x64, with managed library, media import, non-destructive edit, user-installed virtual audio devices, microphone + sound mixing. Direct downloads from GitHub or a future website, not app stores.

- `CLAUDE.md`: short operational contract
- `.claude/rules/`: language/security/testing/Git rules
- `.claude/skills/`: reusable workflows
- `.claude/agents/`: focused read-only helper agents
- `.claude/settings.json`: guarded permission defaults and Bash hook
- `docs/`: product and architecture decisions, engineering policies

**Security note:** Claude hooks are defense in depth, not a sandbox. Review `.claude/settings.json` and commands before use. Real OS-level permissions and GitHub protection are required.
