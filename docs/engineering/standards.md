# Shared engineering standards

These rules apply to human contributors, Claude Code, and Codex. Accepted ADRs in `docs/architecture/decisions/` take precedence over summaries here. Do not edit an Accepted ADR or shipped SQLite migration without owner approval.

## Architecture and code

- Preserve the modular monolith and feature-based React structure (`src/app`, `src/features`, `src/shared`). `shared` must not import `features`.
- Keep the Rust domain independent of Tauri, SQLite, platform APIs, and UI DTOs. Commands validate inputs and call application services.
- Rust owns audio playback, mixing, device access, decoding, persistent storage, and media import. UI does not perform real-time DSP.
- SQLite is the persistent source of truth. Migrations are forward-only, versioned, tested, and approved before creation. Managed media is immutable; import via staging, validation, hashing, atomic move, and reconciliation (ADRs 0002, 0005, 0007).
- Use generated typed IPC commands and DTOs; use events for asynchronous state changes. Never hand-edit `src/shared/ipc/bindings.ts` (ADR 0004).
- Keep real-time callbacks bounded: no file/network I/O, logging, blocking locks, unbounded queues, or steady-state allocation. Account for device changes and stream recovery (ADR 0006).
- Use strict TypeScript and typed Rust errors. Avoid broad assertions, duplicated DTOs, and undocumented panics.

## UI and tests

- Follow ADRs 0008, 0011, and 0012: translated user text in English and Ukrainian, Tailwind semantic tokens, shadcn/ui primitives on Base UI, and Vitest component tests. Do not add a second UI library or router without approval.
- Use semantic controls, visible focus, keyboard navigation, accessible labels, and appropriate loading, empty, error, and disconnected states. Consider narrow windows, high-DPI displays, contrast, and reduced motion (`docs/design/design-system.md`).
- Test behavior at the lowest useful layer. Cover negative paths for high-risk audio, import, database, and device work. Do not update snapshots blindly.
- Run relevant formatting, typecheck, tests, builds, and lint where configured. Follow `docs/engineering/definition-of-done.md`; platform, hardware, signing, and Discord claims require evidence from those environments.

## Security

- Treat filenames, media metadata, URLs, and subprocess output as untrusted. Validate schemes, redirects, private-network destinations, and resource limits for URL import; avoid shell command construction from user input.
- Keep Tauri and OS permissions narrow. Microphone capture must be explicit and user-controlled. Never reveal signing material, tokens, private keys, user audio, or sensitive paths in logs or output.
- Never expose credentials to untrusted PR workflows. Follow `docs/engineering/release-policy.md` for signing and distribution.
