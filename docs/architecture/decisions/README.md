# ADR register

Accepted product decisions from discovery:
- 0001 Desktop stack and modular monolith
- 0002 Managed library and SQLite
- 0003 External virtual device routing
- 0004 Typed IPC contract with tauri-specta (owner chose option B)
- 0005 SQLite access with rusqlite and rusqlite_migration
- 0008 Localization and translatable errors (English and Ukrainian, i18next)
- 0009 Backend-owned file selection (native dialog opened from Rust)

Proposed (awaiting owner decision):
- 0006 Audio engine threading, buffering, callback safety and device lifecycle
- 0007 Managed library import: atomicity, cleanup and recovery

Future ADRs required for: URL providers, FFmpeg distribution, non-destructive edit schema, license and signing provider.
