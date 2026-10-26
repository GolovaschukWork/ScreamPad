# ADR 0013: Microphone and Virtual Output Routing

- Status: Proposed
- Date: 2026-10-25
- Owner: project owner

## Context

ScreamPad currently plays library clips to one system-default output. ADR 0003 requires microphone plus clip routing to a user-installed virtual device for the MVP. ADR 0006 explicitly excludes microphone capture, monitoring and a second output clock. Those are separate streams with independent clocks even when their nominal sample rates match. Sending microphone audio to local speakers can produce feedback. The owner approved an isolated macOS prototype and this Proposed ADR; production integration still requires a separate architecture decision.

## Options

- **A: a bounded microphone queue, with the virtual-output callback as the sink clock.** Capture converts input to mono `f32` and pushes frames to a fixed-capacity SPSC ring. The virtual-output callback resamples from that queue, mixes in a test clip, and writes to BlackHole. A separate output callback plays only the clip on an explicitly selected physical device. This is the prototype approach.
- **B: use the input callback as master and push mixed output into both outputs.** The independently clocked outputs can diverge, so each needs its own bounded queue, resampling and recovery policy. This adds queues and latency without making capture more reliable.
- **C: use the system default output for monitoring and virtual routing.** Rejected: the default may itself be BlackHole and create a duplicate route. Physical monitoring must be separately selected and its device identity checked.

## Proposed decision and prototype design

Use A for the prototype in `src-tauri/examples/mic_route_probe.rs`. It is a standalone Cargo example; no production engine, UI, IPC, SQLite, capability or dependency changes are part of this PR.

- `list` enumerates device names, IDs and reported types. `start --virtual NAME --monitor NAME --mode mic|clip|both --seconds 600` is the explicit capture command. The default microphone is opened only in `mic` or `both` mode after `start`; the system default microphone is never changed.
- The virtual output must have BlackHole in its name. The optional monitor must have a different device ID and CPAL must classify it as speaker, headphones, headset or earpiece. Missing, identical, virtual and unclassified outputs disable monitoring with a message. This conservative rule may disable a usable device when CPAL metadata is incomplete.
- Capture maps any supported input channel count to mono by averaging and converts `f32`, `i16` or `u16` to float. BlackHole gets the mono mic plus a prebuilt clip in its first two channels; any additional channels are silent. The monitor gets the clip alone. Output supports the same three sample formats. Other formats fail before streams start.
- The capture callback pushes to a 200 ms bounded `rtrb` queue. When full, the newest frame is dropped and counted. The virtual callback outputs silence on underflow and counts it. It targets 40 ms queue occupancy, adjusts its consumption ratio by at most 0.5% from nominal, and discards old queued frames down to the target if occupancy exceeds 75% of capacity. This bounds latency but may cause audible discontinuities; quality is a subject of the measurement, not an accepted production algorithm.
- Callbacks use atomics for counters and error notification. They do not allocate, block, log or access files. A device error sets a flag; callbacks become silent immediately, then the control loop drops all streams within its one-second poll. Normal completion also drops all streams and releases the ring.
- A prebuilt ten-second test clip contains five seconds of 523.25 Hz tone followed by five seconds of silence. It repeats to distinguish clip audio from the microphone. It is not a library media file.

## Measurements and current evidence

`cargo test --example mic_route_probe --locked` passes 10 prototype unit tests on macOS arm64, including a ten-minute *simulated* 1% clock mismatch. An earlier `list` and `system_profiler` run returned no devices in that execution environment; it was not a real-device measurement.

On 2026-10-26, a new read-only inventory on macOS 26.2 arm64 returned the following. CPAL `list` reported the IDs and types; `system_profiler SPAudioDataType -detailLevel mini` reported the channels, current nominal sample rates and built-in transport. The commands did not open any streams or request microphone access.

| Device | CPAL ID | CPAL type | System-reported role and configuration |
| --- | --- | --- | --- |
| iPhone Microphone | `coreaudio:<device-specific ID>` | `Unknown` | Input, 1 channel, 48 kHz; transport unknown |
| MacBook Pro Microphone | `coreaudio:BuiltInMicrophoneDevice` | `Unknown` | Default input, 1 channel, 48 kHz; built-in |
| MacBook Pro Speakers | `coreaudio:BuiltInSpeakerDevice` | `Unknown` | Default output, 2 channels, 48 kHz; built-in |

The device-specific iPhone identifier is redacted from this public record; it is not needed to interpret the routing result.

At the time of this initial inventory, no virtual output appeared and no routing stream or microphone capture was started. The owner later installed BlackHole 2ch and restarted CoreAudio. The built-in speakers have CPAL type `Unknown`; under the prototype's conservative monitor rule, they are rejected even though macOS reports them as built-in. This is a concrete limitation to resolve before claiming the local-monitor scenario on this Mac.

After the CoreAudio restart, CPAL enumerated `BlackHole 2ch` as both input and output with ID `coreaudio:BlackHole2ch_UID` and type `Unknown`. macOS reported virtual transport, two input and two output channels at 48 kHz. This shows why CPAL's type alone is insufficient for provider-neutral identification; this prototype still requires BlackHole in the output name.

On the same Mac, the isolated probe completed a 600-second `both` run with the default `MacBook Pro Microphone` routed to `BlackHole 2ch`. The requested `MacBook Pro Speakers` monitor was disabled before stream startup because CPAL classified it as `Unknown`; no monitor callback ran.

| Measurement | Observed result |
| --- | --- |
| Input stream | MacBook Pro Microphone, `f32`, mono, 48 kHz; supported buffer range 15–4096 frames; stream-reported buffer 512 frames |
| Virtual output stream | BlackHole 2ch, `f32`, stereo, 48 kHz; supported buffer range 15–4096 frames; stream-reported buffer 512 frames |
| Microphone queue | 9600-frame capacity (200 ms at nominal rate); maximum occupancy 2560 frames; final occupancy 2176 frames |
| Queue counters | Overflow 0; underflow 0; discarded frames 0 |
| Frame and callback counters | Captured 28,814,336 frames; consumed 28,812,160 frames; input callbacks 56,278; virtual callbacks 56,273; maximum callback size 512 frames for both streams |
| Clock difference reported by the probe | 0–512 input-frame units at the one-second samples; final 512. This counter is callback-quantized and does not prove synchronized hardware clocks. |
| Shutdown | Capture stopped; both streams dropped and the queue released. No device error was observed. |

The maximum queue occupancy corresponds to 53.3 ms of queued frames at 48 kHz; it is **not** an observed end-to-end latency. No end-to-end latency was measured. The one-second occupancy snapshots ranged from 1663 to 2524 frames; the maximum counter records a higher peak between snapshots. The control loop produced 598 periodic rows over the 600-second run, so periodic snapshots are not a continuous trace.

In a separate ten-second clip-only run, FFmpeg's AVFoundation capture of the BlackHole input, with `silencedetect` and null output, observed a roughly five-second non-silent interval followed by silence. This verifies that the generated clip reached the BlackHole input. No audio recording was saved. It does not establish reception by Discord.

During the combined 600-second run, the owner selected BlackHole as Discord's input and reported hearing **only the microphone**, not the test tone. Thus microphone delivery to Discord was observed, but combined microphone-plus-clip delivery to Discord is **NOT VERIFIED**. Whether Discord voice processing suppressed the tone is an untested hypothesis. Local monitoring of the clip is also **NOT VERIFIED**, because the probe did not open a monitor stream.

## Validation still required

1. Repeat Discord checks in separate microphone-only, clip-only and combined runs using a representative sound effect or short speech clip, with the named Discord voice-processing settings recorded. The 523.25 Hz test tone was not heard in the owner's combined test. Confirm that microphone samples stop reaching Discord after capture stops.
2. Resolve physical-monitor selection using reliable role/identity evidence, then verify that local monitoring contains only the clip. Do not route the microphone to speakers or reuse the virtual output as the monitor.
3. Measure end-to-end latency with an actual capture/measurement method if needed; do not infer it from queue depth or buffer sizes. Exercise device failure and recovery on real hardware.

As of 2026-10-26, combined and clip-only Discord routing, local monitoring, capture-stop behavior in Discord, actual observed latency, Windows 11 and Intel macOS are **NOT VERIFIED**. The real ten-minute macOS arm64 queue measurement above is complete; it is not a production routing validation.

## Production integration path and risks

After owner acceptance of this ADR and successful real-device measurements, add an input and selected-output port beside the existing default-output engine, keeping the callback rules of ADR 0006. Define typed device selection and state/events, explicit capture start/stop and permission UX, and a separate approved settings migration only if device choices need persistence. Test stream recovery, drift and feedback on Windows and both Mac architectures before calling the route reliable. The current prototype's linear interpolation, 40 ms target and CPAL device classification are provisional; compare audible quality and queue trends before choosing production resampling and monitoring policy. No first-party virtual driver is proposed.
