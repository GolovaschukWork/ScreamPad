# ADR 0007: Managed Library Import: Atomicity, Cleanup And Recovery

- Status: Proposed
- Date: 2026-10-25
- Owner: TBD

## Context
ADR 0002 copies imported sounds into an application-managed library and indexes them in SQLite. A SQLite transaction does not
cover the file system, so an import that touches both can be interrupted between the two (crash, power loss, full disk,
removed drive, antivirus lock, user cancel) and leave a file without a row or a row without a file. The owner required these
rollback, cleanup and recovery rules to be defined before the managed library is implemented.

Schema 1 (ADR 0005) already enforces: `content_hash` is the unique SHA-256 of the original bytes, `managed_path` is a unique safe
relative storage key, and media columns are immutable. Managed files are immutable after a successful import.

## Options
- **Option A: staging file, durable move, then database commit; reconcile at startup** (proposed).
- **Option B: insert the row first, then copy.** Rejected: the UI could show a sound whose file does not exist, and a crash
  leaves a dangling row that looks valid.
- **Option C: store the audio in the database as blobs.** Rejected: contradicts ADR 0002 and makes large libraries, backups and
  streaming worse.

## Decision

### Layout (under the library root inside the application-data directory)
- `staging/`: scratch files only. Safe to delete at any time when no import is running.
- `media/<first two hash characters>/<hash>.<ext>`: managed files. The storage key equals `managed_path`.
- Both directories sit on the **same volume** so the final move is an atomic rename. If the rename would cross volumes it fails;
  there is no copy fallback.

### Import steps (one import worker; imports are serialized through a bounded queue)
1. **Validate the source.** Open the user-selected path once and inspect the **opened handle**: it must be a regular file (not a
   directory, device or pipe) within the size limit. Symlinks chosen by the user are followed. No path is built from user text.
2. **Copy and hash in one pass** into `staging/<random>.part` (created with owner-only permissions), streaming SHA-256 over the
   bytes actually written. The hash therefore describes exactly what is stored. Compare the source size and modification time
   before and after; a difference fails the import with `SourceChanged`. Call `sync_all` on the staged file.
3. **Inspect the staged file by content, not extension** (`symphonia` probe): detect the container and codec, duration, sample
   rate and channels. Reject unsupported, empty, truncated or oversized input with a typed error and delete the staged file. The
   managed extension comes from a whitelist keyed by the detected format, never from the original file name.
4. **Duplicate check.** If a row with this `content_hash` exists, delete the staged file and return the existing sound.
5. **Durable move.** Create `media/<aa>/`, rename the staged file to its final key, mark it read-only, and `fsync` the parent
   directory on Unix. If the final file already exists (an orphan from an earlier crash), verify its size and hash; if they match,
   discard the staged copy and adopt the existing file, otherwise fail with `LibraryInconsistent` and keep both.
6. **Commit.** `INSERT ... ON CONFLICT(content_hash) DO NOTHING RETURNING id` in one transaction. A conflict means a duplicate that
   appeared meanwhile: treat as step 4. The file is durable before the commit, so a committed row never points at a file that
   could be lost with the same power failure.

### Failure points and outcomes
| Interruption | State left behind | Recovery |
| --- | --- | --- |
| Before step 5 (copy, hash, probe, full disk, source removed, cancel) | a `.part` file in `staging/` | The import deletes it on error; startup deletes every file in `staging/` |
| Between step 5 and step 6 (crash, power loss) | a managed file with no row ("orphan") | Re-importing the same source adopts it (step 5). Startup moves orphans older than a grace period (24 h, a constant to confirm) to deletion; they are copies, the user's source files are never touched |
| After the commit, before the UI is told | consistent | Nothing to do; the sound simply appears |
| Delete flow | row removed first, file second | A crash between them leaves an orphan, handled above. Never delete the file first |
| Row exists, file missing (user or antivirus removed it) | dangling row | Detected by the startup scan; playback returns a typed `MediaMissing` error and the UI offers to re-import or remove the entry. No schema change is needed |
| Row exists, file changed or damaged | wrong bytes | Size is compared at startup; the SHA-256 is verified when a clip is first loaded in a session and on an explicit "verify library" action. A mismatch returns `MediaCorrupt` and the file is never silently repaired |
| Database commit lost after power failure (`synchronous = NORMAL`) | file present, row missing | Same as the orphan case; no data corruption |

### Startup reconciliation
Runs in the background after migrations, never blocking the UI and never on the audio thread: (1) clear `staging/`; (2) list
`media/` and handle orphans; (3) `stat` the file of every row and keep the result in memory. It reports counts, not paths.
Reconciliation only deletes files this application created in `staging/` or orphans past the grace period.

### Concurrency
Imports are serialized on one worker, so two imports of the same file cannot race. The unique constraints stay as the safety net;
a violated one is handled as a duplicate, never as a crash.

### Security and privacy
- All file names, metadata tags and decoder output are untrusted. `original_file_name` is stored as a bare, length-limited file name
  (no directories); the source path is never stored or logged. Error values reaching the UI carry no paths (ADR 0005).
- The import limits (maximum file size, maximum duration, maximum declared sample count) are checked before allocating, and are
  placeholders until measured.
- Managed files are opened read-only; the application never rewrites them.

### Platform notes (not verified)
- Windows: antivirus or indexers may hold a file open briefly, so the final rename retries a few times with a short back-off;
  read-only attributes and open-file deletion behave differently from Unix.
- Directory `fsync` is skipped on Windows. Behavior on network drives and removable media is out of scope for the managed library
  (it lives in the local application-data directory).

## Consequences
- No state in which the UI shows a sound without a file, apart from files removed afterwards, which are detected and reported.
- Orphans and staging files are possible after a crash but are bounded and self-cleaning.
- Extra complexity: a file-system port, a worker with a queue, and a startup reconciliation task.
- Users with many gigabytes of audio pay one full read per imported file (hash plus probe); this is intended.

## Validation
- Failure-injection tests through a file-system port with failpoints at every numbered step and at each table row above; after each
  injected failure and a simulated restart, assert these invariants: every row has a file of the recorded size; `staging/` is
  empty; no orphan outlives the grace period; re-importing the same source ends with exactly one row.
- Negative-path tests: empty, truncated, corrupt, non-audio, wrong-extension (for example MP3 named `.wav`), oversized, directory,
  unreadable and symlinked sources; a source that changes during the copy; a full disk (limited temporary volume where available).
- Duplicate import, concurrent submission of the same file, and an orphan present at the final key.
- Windows 11 and Intel macOS runs in CI; real removable-media, antivirus and power-loss behavior are NOT VERIFIED until tested on
  hardware.
