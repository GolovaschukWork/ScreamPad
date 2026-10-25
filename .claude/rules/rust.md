---
paths:
  - "src-tauri/**/*.rs"
---
# Rust rules
- Commands validate external inputs and delegate to application services; they do not host business logic.
- Domain does not depend on Tauri, platform APIs, SQLite, or UI DTOs.
- Use Result and typed errors; avoid panic, unwrap, expect in production paths unless invariants are proven and documented.
- Audio callbacks: no disk/network I/O, sleeping, blocking locks, unbounded channels, logging, or allocation on steady-state hot paths.
- Device disconnection, device changes, format conversion, overflow and underrun require explicit recovery strategies.
- Favor measurable performance improvements over premature DSP optimization.
- Migrations must be forward-only, versioned, tested, and explicitly approved.
