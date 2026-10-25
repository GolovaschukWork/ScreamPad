# Real-time audio design constraints

## Targets (proposed; benchmark on real hardware)
- Prefer a consistent internal processing format (e.g. 48 kHz float frames), negotiate devices explicitly.
- Preallocate steady-state audio buffers; keep callback bounded and non-blocking.
- Separate decoder workers and PCM buffering from time-critical callbacks.
- Avoid allocations, file/network I/O, mutex contention and logging in callbacks.
- Support buffer underflow/overflow metrics outside callback, device re-selection, mic mute, monitor mute, safe shutdown and reconnect.

## Acceptance scenarios
1. Mic only, clip only, and combined signal audible in a voice-chat test call.
2. Selected virtual device disconnects and reconnects without process crash.
3. App minimizes to tray and global shortcut playback still works.
4. Monitoring does not unexpectedly create speaker-to-microphone feedback.
5. Verify Windows 11 x64 and macOS 14+ on both Apple Silicon and Intel test targets.

**Not yet measured:** latency, glitch rate, CPU budgets, Discord integration; do not state numerical guarantees before benchmarks.
