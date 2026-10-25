# ADR 0006: Audio Engine Threading, Buffering, Callback Safety And Device Lifecycle

- Status: Accepted (owner decisions of 2026-10-25: `cpal`, `rtrb 0.4.0`, `rubato 5.0.1`, `panic = unwind`); mixer and clip decoding implemented, device engine pending
- Date: 2026-10-25
- Owner: TBD

## Context
The first audio vertical slice plays an imported clip through one output device with `cpal` (audio I/O) and `symphonia`
(decoding), both chosen by the owner. `docs/architecture/audio-engine.md` sets the constraints: a consistent internal format,
preallocated steady-state buffers, a bounded non-blocking callback, decoder workers separated from the callback, and explicit
device restart and shutdown handling. This ADR turns those constraints into a design that can be implemented and tested.

Out of scope for the slice and for this ADR: microphone capture, live mixing, virtual-device routing, monitoring, FFmpeg, URL
import. The design must not make them impossible; each needs its own ADR (a second stream means a second clock domain).

Facts checked against `cpal` 0.18.2 (changelog) and crates.io on 2026-10-25:
- `cpal` 0.18.0 changed `build_*_stream` so streams **do not start automatically**; `play()` is explicit.
- Error callbacks report `DeviceNotAvailable`, `StreamInvalidated` (CoreAudio sample-rate change, ASIO resync), xruns
  (0.18.2, on several backends) and, in 0.18.0, `DeviceChanged`. The WASAPI `DeviceChanged` behavior was reversed in 0.18.2
  and the unreleased changelog reverses it again, so **behavior differs by platform and version and must not be relied on
  alone**.
- There is no general hot-plug notification API.
- `cpal`'s optional `realtime` feature can raise the audio thread priority and may report `RealtimeDenied`.
- `symphonia` 0.6.1 is MPL-2.0 and every codec and container is a Cargo feature; `cpal` 0.18.2 is Apache-2.0.

## Options
- **Option A: control thread + dedicated audio-host thread + wait-free queues + preloaded PCM** (proposed).
- **Option B: share state between control code and the callback with `Mutex`/`RwLock`.** Rejected: blocking locks are
  forbidden in the callback.
- **Option C: stream every clip from disk through the callback.** Rejected: file I/O in the callback is forbidden, and
  streaming needs the decoder-worker design anyway.
- **Option D: run the decoder in a separate process.** Not for the first slice; kept as the mitigation if in-process decoding
  proves unsafe (see Risks).

## Decision

### Threads and ownership
| Thread | Owns | Rules |
| --- | --- | --- |
| UI / IPC (Tauri runtime) | nothing audio-related | sends control messages to the engine; never blocks on audio |
| Engine control (one thread, non-real-time) | clip cache, voice-id allocation, the device state machine, metrics | all blocking work lives here: decoding calls, resampling, device enumeration, stream rebuilds |
| Decoder workers (bounded pool) | one decode job at a time | read file, decode with `symphonia`, resample, return an immutable clip buffer; failures become typed errors |
| Audio host thread | the `cpal` stream and the mixer state | `cpal` streams are not `Send` on every platform, so the stream is created, played and dropped on this thread; it only executes control-thread requests |
| Audio callback (OS real-time thread) | the mixer render function | real-time rules below |

The engine belongs to the backend application state, not to the webview, so it keeps running when the window is hidden (tray
mode).

### Internal format and clips
- Internal format: `f32`, interleaved, channel count and sample rate of the opened device (typically 48 kHz stereo).
- Decoding and resampling happen **before** playback, off the callback. A clip is an immutable `Arc<[f32]>` plus metadata.
  Preloaded clips give immediate triggering, which matters for a soundboard.
- Resampling to the device rate is needed for most files (44.1 kHz to 48 kHz). A resampler crate (for example `rubato`) is
  proposed and needs owner approval; a hand-written linear interpolator is the quality-poor fallback.
- Memory arithmetic: stereo `f32` at 48 kHz costs about 0.38 MB per second, so a 30 s clip is about 11.5 MB and a 5 min clip
  about 115 MB. The slice therefore caps preloaded clip length (the limit is set from measurement, not guessed here) and
  rejects longer files with a typed `ClipTooLong` error. Streamed playback of long files is a later decision.

### Callback contract
The callback reads commands, renders and returns. In the callback there is **no**: allocation or deallocation, locking,
sleeping, file or network I/O, logging, panics, `Instant::now()` in production builds, or channel operations that can block.
- Commands (`Play { voice, clip, gain }`, `Stop`, `StopAll`, `SetMasterGain`) arrive through a **bounded wait-free
  single-producer single-consumer queue** (for example `rtrb`, needs approval; `ringbuf` is the alternative). Capacity is fixed
  (initially 256). If it is full the control thread gets a `Busy` error; nothing is queued without bound.
- The mixer preallocates a fixed voice array (initial maximum 32 voices). A `Play` beyond the limit is rejected with an event,
  not allocated.
- **Deallocation hazard:** when a voice finishes, the callback must not drop the last `Arc` of a clip. Finished voices move
  into a preallocated "retired" queue that the control thread drains and drops. The same applies to replaced clips.
- Events back to the control thread (`VoiceFinished`, `CommandRejected`) use a second bounded wait-free queue. Counters
  (callbacks, xruns, rejected commands) are atomics updated with relaxed ordering.
- Mixing is a sum of voices times gain, followed by a limiter or clamp so overload never produces NaN or out-of-range
  samples. Arithmetic avoids indexing that can panic; the render function is written so bounds are proven or checked without
  `unwrap`.
- A panic in the callback would abort the process (`panic = "abort"` in the release profile), so the callback code must be
  panic-free by construction and by test.

### Buffering and latency
- Start with `BufferSize::Default` and record the callback size reported by the stream (`cpal` 0.18 `buffer_size()`).
  Requesting a fixed small size is a later, measured optimization.
- **No latency, glitch-rate or CPU numbers are promised.** They come from the benchmark task (open question 9).
- The `realtime` feature is enabled only if measurements show a benefit; `RealtimeDenied` is a warning, never a fatal error.

### Device lifecycle
States: `Closed → Opening → Running → Recovering → Running | Failed`.
- Selection policy: "System default" follows the default output device; a specific device is **not** silently replaced if it
  disappears. The engine reports `DeviceLost` and waits for the user's choice or the device returning.
- The `cpal` **error callback only records an event** (queue push or atomic flag) and wakes the control thread. It never
  rebuilds streams itself.
- Triggers for `Recovering`: `DeviceNotAvailable`, `StreamInvalidated`, and `DeviceChanged` where reported.
- Because reporting differs by platform and version, the control thread also **polls** the default output device identity
  (on the order of once per second, off the callback) and rebuilds when it changes. The poll interval and whether polling is
  needed per platform are verified on real hardware.
- Rebuild uses bounded retry with backoff; after the limit the state is `Failed` and the UI shows a typed error with a retry
  action. In-flight voices are dropped on rebuild (playback restarts only on a new user action).
- Xruns are counted and surfaced as metrics, not as user errors.
- Shutdown: stop accepting commands, `pause`, join the audio-host thread, then drop the stream; no thread outlives the process.
- Sample-rate or channel-count change after a rebuild invalidates the preloaded clip cache (clips are re-resampled lazily).

### Decoding safety
- All media is untrusted input (security rules). Decoder workers return typed errors for corrupt, truncated, unsupported or
  oversized input and enforce limits on duration, sample count and decoded size before allocating.
- Enable only the `symphonia` codec and format features the product needs (initial proposal: WAV/PCM, FLAC, MP3, Ogg Vorbis,
  and MP4/AAC for audio extracted from common containers); avoid `all`. The exact list is confirmed in the implementation PR.
  **Chosen for the import PR (2026-10-25):** `symphonia =0.6.1` with `default-features = false` and the features `wav`, `pcm`, `flac`, `mp3`, `ogg`, `vorbis`, `isomp4`, `aac`, `id3v2` (the last so MP3 files with ID3 tags are recognized). The library is MPL-2.0; its compatibility with the project license stays part of the license audit.

### Ports and code layout
- A small `AudioBackend`/`OutputStream` port in `audio` hides `cpal` so the engine supervisor can be tested against a fake
  backend. `domain` and `application` do not depend on `cpal` or `symphonia`.
- The mixer is a plain struct with a pure `render(&mut self, out: &mut [f32])` method, testable without any audio device.

## Consequences
- Predictable, bounded real-time behavior and a testable core; the cost is more moving parts (several threads, queues, a
  retire path) than a naive design.
- New dependencies need owner approval before implementation: `cpal` and `symphonia` (approved as stack, exact versions and
  features to be confirmed), plus proposed `rtrb` (or `ringbuf`) and a resampler.
- Preloading bounds the clip length and uses memory proportional to the library in use; a cache budget with eviction is
  required.
- **License:** `symphonia` is MPL-2.0 (file-level copyleft). Compatibility with the project license is part of the pending
  license and dependency audit; this ADR does not settle it (not legal advice).
- Behavior after unplugging or switching devices differs per OS and `cpal` version, so the lifecycle code needs real-device
  verification on Windows 11 and macOS before it can be called reliable.

## Implementation notes (mixer and clip decoding, 2026-10-25)
Implemented in `src-tauri/src/audio` and `infrastructure/media/decode.rs`; the `cpal` stream, the device state machine and the
IPC commands are the next step.

- **Dependencies (owner-approved, exact pins):** `rtrb =0.4.0` for the wait-free queues, `rubato =5.0.1` for resampling
  (`Fft` resampler with `process_all`, which trims the filter delay). Both are `MIT OR Apache-2.0`.
- **Mixer:** 32 voices in a fixed array; three bounded queues (commands 256, events 256, retired clips 128). `render` sums the
  voices, applies the master gain once, and clamps; NaN and infinite values become silence. A finished voice hands its
  `Arc<Clip>` to the retired queue and the control thread frees it. If the control thread stops draining, ended voices keep their
  slot, and a rejected clip that has nowhere to go is leaked and counted (`leaked_clips`) instead of being freed in the callback.
- **Decoding:** a clip is decoded at the file's channel count, resampled to the device rate, then fitted to the device channel
  count (mono is copied, several channels are averaged into mono, surround keeps only its front pair). The clip length limit is a
  caller-supplied cap, about 23 MB per minute of stereo at 48 kHz.
- **Known limitations:** `symphonia` does not trim the encoder delay of MP4/AAC (its table lists gapless as unsupported for
  ISO/MP4), so an AAC clip starts with silence of roughly 1000 source frames; this must be trimmed before AAC counts as an
  instant-trigger sound. Surround downmix is not implemented. Stopping a voice cuts it abruptly (no fade), which can click.
- **Evidence:** 23 mixer tests with deterministic input; `tests/audio_realtime.rs` installs a counting global allocator and
  asserts **zero allocations and zero deallocations** inside `render` over 600 rounds that include rejections, stops and clips
  whose only owner is the voice. It was mutation-checked: allocating inside `render`, or dropping a clip there, makes it fail.

## Risks and decisions for the owner
1. **Panic strategy (decided by the owner on 2026-10-25): keep the default `panic = "unwind"`.** The Tauri template's
   `panic = "abort"` was removed from the release profile. Expected failures use `Result`; production code does not `unwrap()` or
   `expect()` unverified external data. Decoding still runs behind `catch_unwind` in workers, with malformed-file tests, because
   third-party decoders can panic on hostile input.
2. Dependencies: approved by the owner on 2026-10-25 (`rtrb 0.4.0`, `rubato 5.0.1`).
3. The clip-length cap and voice limit are placeholders until measured.

## Validation
- Mixer unit tests with deterministic input: summing, gain, clamp, voice limit, finish and retire behavior, command overflow.
- **Allocation test:** run the render function under a counting global allocator in a test binary and assert zero allocations
  and deallocations after warm-up.
- Fake-backend tests for the state machine: device lost, default device changed, stream invalidated, rebuild failure with
  backoff, shutdown during recovery.
- Negative decode tests: empty, truncated and corrupt files and oversized declared lengths return typed errors without panics.
- Manual real-device checklist (macOS arm64 and Intel, Windows 11): playback, unplugging and replugging the output, changing
  the system default device, sample-rate change, sleep and wake. Until it is run on that hardware the lifecycle is **NOT
  VERIFIED**, and so are latency, glitch rate and CPU use (`audio-engine.md`: not yet measured).
