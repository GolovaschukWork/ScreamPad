# ADR 0006: Audio Engine Threading, Buffering, Callback Safety And Device Lifecycle

- Status: Accepted (owner decisions of 2026-10-25: `cpal`, `rtrb 0.4.0`, `rubato 5.0.1`, `panic = unwind`); mixer, clip decoding, the device engine and the playback commands implemented; manual device verification pending
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
- **Known limitations:** surround downmix is not implemented (the front pair is kept).
- **Evidence:** 23 mixer tests with deterministic input; `tests/audio_realtime.rs` installs a counting global allocator and
  asserts **zero allocations and zero deallocations** inside `render` over 600 rounds that include rejections, stops and clips
  whose only owner is the voice. It was mutation-checked: allocating inside `render`, or dropping a clip there, makes it fail.

## Implementation notes (device engine, 2026-10-25)
Implemented in `audio/backend.rs`, `audio/engine.rs` and `infrastructure/audio/cpal_backend.rs`.

- **One engine thread** owns the backend, the stream and the mixer's control handle. This merges the "engine control" and "audio
  host" threads of the design: decoding happens in the caller, so the control work left (drain, poll, rebuild) fits the thread that
  must own the stream anyway. The backend is created on that thread, so a non-`Send` platform stream never crosses threads.
- **State machine:** `Starting`, `Running`, `Recovering { attempt }`, `Failed(reason)`, `Stopped`. Retryable open failures (no
  device, busy) back off over a configurable list (production: 0.25, 0.5, 1, 2, 4 s) and then end in `Failed`; permission and
  unsupported-format failures fail at once. `retry()` and a newly appearing default device recover a `Failed` engine.
- **Stale events are ignored.** Every stream gets a generation number and every event carries it. Found by a load test: late error
  callbacks of a replaced stream caused needless rebuild cascades. A burst of errors from one stream now causes one rebuild.
- **A failure that cannot be retried stays failed.** Found by a test: the engine had not recorded which device a failure was about,
  so the first periodic check saw a "new" default device and retried a permission failure. The identity is now recorded on failure.
- **Default-device changes** are detected by an event hint and by polling the device identity (production: every second), because
  `cpal` 0.18.2 does not report them uniformly across platforms.
- **Sample format:** the engine mixes in `f32` and asks `cpal` for a float configuration; a device that offers none is
  `Failed(Unsupported)`. Other sample formats are not converted yet.
- **Counters** (xruns, device losses, opens, rejected plays, callbacks) are exposed in `EngineSnapshot`.
- **Evidence:** 18 engine tests against a fake audio API (recovery with a different format, backoff, giving up, retry, permanent
  failures, polling, bursts, stale events, release of finished clips, shutdown) stress-run 96 times, 16 in parallel, without a
  failure; `tests/audio_device.rs` (ignored by default) ran on macOS arm64 against the real default output: 48 kHz, 2 channels,
  94 callbacks, no xruns, no device loss.
- **Not verified:** unplugging, switching the default device, sample-rate changes, sleep and wake, Bluetooth, and everything on
  Windows 11 and Intel macOS. The manual checklist is `docs/engineering/audio-manual-checklist.md`. Latency, glitch rate and CPU
  are not measured.

## Implementation notes (playback service and commands, 2026-10-25)
Implemented in `application/playback.rs`, `application/clip_cache.rs`, `commands/playback.rs` and `features/library`.

- **Playing a sound** (`PlaybackService::play`): read the output format from the engine, take the decoded clip from the cache or
  prepare it, hand it to the engine. Preparing opens the managed file **once**, compares its size with the recorded one, hashes
  those same bytes with SHA-256 against `content_hash` (ADR 0007: verified when a clip is loaded), rewinds, and decodes. Tampered
  bytes never reach the decoder. A missing file is `mediaMissing`; a different size, hash or an undecodable file is `mediaCorrupt`.
- **Cache:** least recently used, with a byte budget (provisional 64 MiB), emptied when the output format changes. Triggering a
  sound again does not decode it again. A clip larger than the budget is played but not cached.
- **Device switch races:** if the format changes between preparing a clip and handing it over, the engine refuses it
  (`FormatMismatch`) and the service decodes it again once, for the new format.
- **Limits (provisional until measured):** clip length 60 s, decode time 30 s, cache 64 MiB.
- **IPC:** `play_sound(id)`, `stop_all_sounds`, `audio_status` and `retry_audio`. `play_sound` first consults the last
  reconciliation, so a file already known to be missing or damaged is reported without touching the disk. New error kinds:
  `audioUnavailable`, `audioBusy`, `soundNotFound`, `playbackClipTooLong`, `playbackDecodeTimeout`, all with English and
  Ukrainian text. The engine starts at launch and is stopped on `RunEvent::Exit`.
- **UI:** a Play button per sound, "Stop all", and a banner for the output state (starting, reconnecting, failed with a reason and
  a retry button). The state is refreshed every two seconds while the window is visible; a typed event can replace polling later.
- **Evidence:** 15 playback tests with real adapters and a fake audio API (cache, format change, device switch race, tampering by
  size, by content with the same size and by a planted link, limits, unavailable output, stop, threads), 5 IPC tests through the
  command layer, and a silent end-to-end test on the real output (`tests/audio_device.rs`, ignored by default).
- **Observed on real hardware (macOS arm64, 11 sessions):** 48 kHz stereo, about 10 ms per callback, one xrun in the very first
  session and none in the following ten. Not enough to state a glitch rate.
- **Not done:** no gain control in the UI (fixed at unity); AAC delay is still untrimmed; sounds are one-shot (no loop,
  retrigger policy or per-sound volume yet); no global shortcuts or tray playback.

## Implementation notes (AAC trimming and soft stop, 2026-10-25)
- **AAC encoder delay is trimmed.** `symphonia` 0.6.1 parses the MP4 `elst` edit list but does not apply it (its source says so),
  and leaves `Track::delay` and `padding` unset for MP4, so an AAC clip began with about a thousand frames of near-silence and
  its duration was overstated. `infrastructure/media/mp4_edit.rs` reads the single plain edit of the audio track with a
  bounds-checked box reader (version 0 and 1, movie time converted to media time) and the probe and the clip decoder drop the
  priming and padding frames. Anything unexpected (several edits, an empty edit, a changed rate, a timescale that is not the
  sample rate, damaged or hostile boxes) means no trim, never a failure. The 0.25 s fixture now decodes to exactly 2000 frames
  at 8 kHz and its duration is 250 ms. Files whose encoder wrote no edit list are not trimmed.
- **Stopping fades out.** A stopped voice ramps linearly to silence over 5 ms (240 frames at 48 kHz, scaled to any rate) instead of
  being cut, which clicks. A voice stopped before it was ever heard is dropped without a sound, stopping twice does not lengthen
  the fade, and the fade runs inside the allocation-free render path (the real-time test covers stops).
- **Evidence:** 8 tests of the MP4 reader including every truncation and every single-byte corruption of a real file; 8 fade tests
  including a check that no step between neighbouring samples exceeds one ramp step, and a mutation check (replacing the fade by a
  cut fails four tests).
- **Observed on real hardware:** the default output of the development Mac ran at 24 kHz stereo in one session and 48 kHz stereo in
  earlier ones; the whole path including resampling worked at both, with no xruns in these runs.

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
