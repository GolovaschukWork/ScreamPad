# Architecture overview

## Boundaries

```text
React UI (app/features/shared)
    | typed commands + controlled events
Tauri command adapters
    | calls
Application services -> Domain models/ports
    |                |
Infrastructure   Rust audio runtime
(SQLite/media)   (mic + sound mixer, devices, outputs)
                      |
              OS audio APIs / external virtual device
```

- The domain is independent of Tauri, storage and device implementations.
- IPC commands are validated and never expose arbitrary filesystem/network actions.
- Backend owns persistent state; frontend caches derived/read state as necessary.
- Audio engine stays alive independent of the window while tray/background mode is active.

## Audio pipeline
Mic capture -> channel normalization/resampling -> gain -> bounded mixer <- decoded clip frames
Mixer -> optional local monitoring and selected virtual output -> Discord selects matching virtual input.

Device restart, feedback/loopback prevention, mute and monitoring must be designed explicitly. Never imply virtual mic installs automatically.

## Data and import
- Managed storage under OS application data directory (bundle identifier TBD).
- Staging -> validate/decode -> content hash -> move into managed storage -> SQLite transaction/reconciliation.
- Edit operations are metadata; decoded/cached peaks are derived artifacts, not the primary source.
- Remote URL imports need allowlist/provider strategy, SSRF prevention, redirect validation and quotas.

## Dependency decisions pending spikes
- Tauri 2 / typed IPC generator compatibility, Rust audio I/O implementation, FFmpeg packaging and legal review, virtual-device workflow on both OSes.
