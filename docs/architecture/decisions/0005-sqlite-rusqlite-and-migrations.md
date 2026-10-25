# ADR 0005: SQLite Access With rusqlite And rusqlite_migration

- Status: Accepted (the owner approved the library choice on 2026-10-25; the design below is confirmed or amended in the implementation PR)
- Date: 2026-10-25
- Owner: project owner

## Context
ADR 0002 makes SQLite with versioned migrations the source of truth for the managed library. This ADR chooses the access
library and defines how the database is isolated from the UI and the audio threads, how schema changes are applied, and how
transactions, rollback and failures are tested.

ScreamPad is a single-user desktop application: one process owns the database file, the data set is small (metadata for
sounds, tags, boards, edit parameters), and statements take microseconds to milliseconds.

## Options
- **Option A: `rusqlite` + `rusqlite_migration`** (synchronous). Chosen.
- **Option B: `sqlx`** (async, built-in migrator, optional compile-time query checking). Rejected for this application.
- **Option C: JSON files.** Already rejected by ADR 0002.

## Decision
Use `rusqlite` with the `bundled` feature and `rusqlite_migration`, pinned exactly in `Cargo.toml`:

| Crate | Version | License | Note |
| --- | --- | --- | --- |
| `rusqlite` | `=0.40.2` | MIT | `0.41.0` exists (2026-10-24) but `rusqlite_migration` 2.6.0 requires `^0.40`, so `0.41` is not usable yet |
| `rusqlite_migration` | `=2.6.0` | Apache-2.0 | `include_dir` feature is **not** used |
| `libsqlite3-sys` | resolved by Cargo (0.38.2 in the spike) | MIT | compiles SQLite from source (`bundled`) |

`bundled` gives every platform the same SQLite version and features instead of the OS copy. It needs a C compiler in CI
(present on the macOS and Windows runners). The `backup` feature is not needed: pre-migration backups use `VACUUM INTO`.

### Why synchronous SQLite fits a desktop application
- SQLite is an in-process library doing blocking file I/O. An async wrapper adds an executor or a thread pool underneath and
  does not make queries faster; for one user and a small data set it only adds complexity.
- A synchronous API is simpler to reason about and to test: a transaction is a scope, rollback is a drop.
- The cost is that a call can block its thread. That is handled by isolation (next section), not by an async driver.
- `sqlx` would add an async runtime dependency, a larger dependency tree, and offline query metadata for compile-time
  checking, with no benefit for a local single-writer database. Its migration checksum verification is replicated by a test
  (see Migrations).

### Isolation from UI and audio threads
- Only the `infrastructure` layer touches SQLite. `domain` and `application` see repository ports (consistent with
  `.claude/rules/rust.md`); no SQL or `rusqlite` types leak upward.
- Connections: one **writer** connection (serialized behind a mutex, used by import and edit use cases) and a small fixed
  number of **read-only** connections. WAL mode lets readers proceed while the writer commits.
- Tauri command handlers never run SQL on the async runtime or UI-facing threads directly: repository calls go through a
  blocking-task boundary (for example `spawn_blocking`) so a slow query cannot stall IPC.
- **The audio thread never touches the database, a connection, or any lock shared with the database path.** Playback receives
  a fully resolved, immutable request (managed file location or decoded frames) through a bounded queue prepared outside the
  callback. Details belong to the audio-engine ADR.
- Connection settings applied on open: `journal_mode = WAL`, `foreign_keys = ON`, `busy_timeout = 5000`,
  `synchronous = NORMAL`. With WAL, `NORMAL` can lose the most recent commit after a power failure but cannot corrupt the
  database; the file-versus-row reconciliation from ADR 0002 repairs a lost commit. `synchronous = FULL` is the fallback if
  that trade-off is rejected.
- All statements use bound parameters; no SQL is built from user input, file names or metadata.

### Migrations
- SQL files live in `src-tauri/migrations/NNNN_description.sql`, embedded with `include_str!` and listed in one ordered
  `Migrations::new(vec![M::up(...)])`. No `down` migrations: forward-only, as required by the Rust rules.
- The schema version is `PRAGMA user_version`. Migrations run at startup, on the writer connection, before any other database
  use.
- Every new migration and every schema change needs explicit owner approval (`CLAUDE.md`).
- **Atomic upgrade**: the spike showed that when a later migration fails, the earlier pending ones are rolled back too
  (`user_version` stays at the old value and no partial table remains).
- **Backup before upgrade**: when the file exists and migrations are pending, create `VACUUM INTO` a backup next to the
  database (`library-pre-v{from}-to-v{to}.db`) and keep the last three.
- **Database newer than the app** (`DatabaseTooFarAhead`): refuse to open it, leave it untouched, and tell the user to update
  the application. Never downgrade silently.
- **Failure at startup** (migration error, corrupt file, locked file): do not continue with a half-initialized library; show a
  typed error in the UI with the backup location. A corrupt file is renamed aside (never deleted) before offering recovery.
- **Shipped migrations are immutable.** `rusqlite_migration` does not store checksums, so a test compares each shipped
  migration file with a recorded SHA-256 and fails when one is edited.

### Testing transactions, rollback and database failures
Layers, from fastest to slowest:
1. **Migration tests (in-memory)**: `Migrations::validate()`, upgrade from empty to latest, idempotence, and an upgrade from a
   committed fixture database for each released schema version, asserting that existing rows survive.
2. **Transaction tests**: use cases run in one transaction; tests force a constraint violation or an injected error halfway
   and assert nothing was written (rollback on drop). Foreign keys and cascades are exercised explicitly.
3. **File-backed tests (temporary directory)**: WAL behavior, backup creation and restore, a corrupt file, a read-only
   directory, a second writer hitting `busy_timeout`, and disk exhaustion simulated with `PRAGMA max_page_count`.
4. **Crash-point tests for import**: inject a failure after the file is moved into managed storage but before the commit, and
   after the commit but before cleanup; reconciliation must restore a consistent library (ADR 0002).
5. All storage errors map to a typed `StorageError` (busy, full, corrupt, too new, migration, I/O); production code does not
   `unwrap` or `expect` on database results.

## Consequences
- Simple, fast, easily tested storage with a small dependency set; no async runtime coupling.
- Blocking calls must stay behind the blocking-task boundary; code review must reject direct SQL on async or audio paths.
- Pinned to `rusqlite 0.40.x` until `rusqlite_migration` supports `0.41`; upgrades are deliberate PRs.
- The immutability test, backup and fixture databases are our responsibility; the migration library does not provide them.
- Rollback of this decision: the repository ports isolate SQLite, so replacing the library rewrites `infrastructure` only.

## Validation
Spike (throw-away crate, macOS arm64, Rust 1.99.1, `rusqlite 0.40.2` `bundled`, `rusqlite_migration 2.6.0`), 8 tests passing:
valid and idempotent migrations; a failed upgrade leaves `user_version = 0` and no partial table; constraint violation rolls
the transaction back; foreign keys enforced with cascade; `VACUUM INTO` backup is readable; a corrupt file returns the error
`file is not a database` without panicking; a newer database is detected (`DatabaseTooFarAhead`); a second writer receives
`database is locked` after the timeout instead of hanging.

Implementation (`src-tauri/src/infrastructure/sqlite`, 21 Rust tests on macOS arm64): the connection settings above, atomic
upgrade, backup with pruning to the newest three, rejection of a newer database, corrupt-file detection with quarantine
(bytes preserved), writer lock timeout mapped to `Busy`, disk exhaustion via `PRAGMA max_page_count` mapped to `Full`,
constraint violations mapped to `Constraint` with transaction rollback, and the shipped-migration checksum test. The bundled
SQLite is 3.53.2 (`libsqlite3-sys` 0.38.2) and a test requires at least 3.35.

Note: `rusqlite_migration` rejects an empty migration list (`NoMigrationsDefined`), so the production database cannot be
opened before the first schema migration is approved and shipped.

NOT VERIFIED: behavior on Windows 11 and Intel macOS until the CI matrix is green for the implementation PR, and performance
on a large library.
