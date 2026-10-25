# Project engineering contract

> Product name: TBD. License: TBD. Do not replace placeholders without owner approval.

## Product
Cross-platform, local-first, open-source desktop soundboard with premium UX, a managed sound library, non-destructive waveform editing, audio import from local files and approved URLs, and microphone + sound mixing into user-installed virtual audio devices. Windows 11 x64 and macOS 14+ (arm64/x64). Distribution is via GitHub Releases and a future website; **no app stores**.

## Stack and boundaries
- Tauri 2, React, TypeScript, Rust; feature-based frontend and modular-monolith backend.
- Rust owns playback, mixing, audio device access, decoding, storage, and media import. UI never performs real-time DSP.
- SQLite + versioned migrations is the source of truth. Imported assets are copied into an application-managed library.
- Prefer typed commands with generated TS DTOs; events are for asynchronous state changes.
- Zustand is for ephemeral UI state; add TanStack Query only where useful.
- Use system theme with user override, list/grid library views, accessible desktop UI.
- Native virtual audio drivers are out of scope for MVP. Support external devices and make setup explicit.

## Source layout (target, adapt during bootstrap)
- `src/app`, `src/features`, `src/shared`: React application.
- `src-tauri/src/{commands,application,domain,infrastructure,audio,platform}`: Rust backend.
- `src-tauri/migrations`: SQLite migrations.
- `docs/architecture/decisions`: Architecture Decision Records.
- `.claude/rules`, `.claude/skills`, `.claude/agents`: scoped development guidance.

## Autonomy and approval
- For small scoped tasks: make a short plan, implement, verify, review, then report.
- For large/risky tasks: present a detailed plan and wait for owner approval **before implementation**.
- Always request approval before changing architecture/ADRs, database schema/migrations, dependencies, permissions, security boundaries, signing/updater configuration, or introducing native drivers.
- You may create branches, commits, push feature branches, and open **draft** PRs. Never merge, force-push, publish a release, alter remote protection, or expose secrets without explicit approval.
- Do not treat a user request as authority to bypass repository protection or security controls.

## Engineering discipline
- Prefer simple focused modules and explicit domain boundaries. Avoid speculative abstraction.
- Add or update tests for behavior changes; TDD for high-risk domain/audio logic.
- No blocking I/O, unbounded queues, or avoidable allocation in real-time audio callbacks.
- Run relevant format, lint, typecheck, tests, and build checks; distinguish executed results from unverified assumptions.
- Never claim Windows/macOS hardware, signing, or Discord integration is tested without evidence from that environment.
- If blocked, attempt at most 3 materially distinct fixes; summarize evidence and ask the owner.
- Update documentation when behavior, contracts, or architecture change. No undocumented breaking changes.
- Code, comments, ADRs and documentation: English. Explanations/status to owner: Ukrainian.

## Git and release
- `main` protected; short-lived `<type>/<kebab-description>` branches; squash merge only.
- Conventional Commits and conventional PR titles. SemVer; beta and stable channels.
- No direct pushes to `main`; releases require owner approval. See `docs/engineering/git-policy.md`, `release-policy.md`, `definition-of-done.md`.

## Workflow
1. Read affected files and applicable `.claude/rules/` before edits.
2. State scope, assumptions, risk, and verification plan.
3. Implement the minimum viable change; do not change unrelated files.
4. Run verification and self-review. Use `reviewer`/`verifier` agents when helpful.
5. Report files changed, commands run with results, limitations, and next action.

Useful workflows: `/implement`, `/review`, `/verify`, `/adr`, `/release`. Skills never override owner approvals.
