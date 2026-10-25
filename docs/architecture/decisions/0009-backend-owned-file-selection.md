# ADR 0009: Backend-Owned File Selection

- Status: Accepted (owner approved `tauri-plugin-dialog` on 2026-10-25)
- Date: 2026-10-25
- Owner: project owner

## Context
Importing a sound needs a path chosen by the user. `docs/architecture/overview.md` requires that IPC commands "never expose
arbitrary filesystem actions". A command such as `import_sound(path)` would let any code running in the webview (a bug, a
compromised dependency, injected content) make the application read any file the user can read and copy it into the library.

## Options
- **Option A: the webview opens the dialog and passes the path to the backend.** Rejected: the backend cannot tell a user's choice
  from a path invented by script.
- **Option B: the backend opens the native dialog and imports what the user chose** (chosen). The command takes no path.
- **Option C: drag and drop of files onto the window.** Possible later; the backend would receive the dropped paths from the
  operating system event, not from script, so it needs its own review.

## Decision
Use `tauri-plugin-dialog =2.8.3` (`Apache-2.0 OR MIT`) from Rust only.
- `import_sound` has no parameters. A `FilePicker` port (`application::ports`) hides the dialog; production uses
  `TauriFilePicker`, which calls `blocking_pick_file` on the blocking pool, and tests use a picker with a fixed answer.
- The window's capability (`capabilities/default.json`) grants **no** `dialog:*` and no `fs:*` permission, so script cannot open a
  dialog or use the file system plugin. An integration test fails if any capability file ever grants one.
- The extension filter in the dialog is a convenience only. The chosen file is validated by its content during the import
  (ADR 0007), so renaming a file never gets it past validation, and a wrong filter never blocks a valid file.
- Cancelling the dialog is a normal result (`null`), not an error.

## Consequences
- `tauri-plugin-dialog` pulls in `rfd` and the `tauri-plugin-fs` crate. The fs plugin is **not** registered, so none of its commands
  exist at run time; it is present only because the dialog plugin depends on it for scope handling.
- The OS dialog title and the "Audio" filter label are not localized (the title is left to the operating system); the dialog
  buttons follow the system language.
- Drag and drop and "import a folder" need their own review before they are added.

## Validation
- Integration tests with a fake picker: import, repeat import (`alreadyInLibrary`), cancelled dialog, unusable files, unavailable
  library, and the capability guard.
- NOT VERIFIED: the real native dialog on Windows 11 and macOS (it cannot be driven by the automated tests).
