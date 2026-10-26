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

No BlackHole or other virtual output appeared in either inventory. Because the prototype requires an output whose name contains `BlackHole`, no routing stream or microphone capture was started. The built-in speakers also have CPAL type `Unknown`; under the prototype's conservative monitor rule, they would be rejected even though macOS reports them as built-in. This is a concrete limitation to resolve before claiming the local-monitor scenario on this Mac.

The prototype logs actual device names, IDs, formats, nominal sample rates, channels, supported and actual stream buffer sizes, maximum observed callback frames, queue occupancy and maximum, overflow, underflow, discarded frames, callback counts and a cumulative capture-versus-virtual-output clock difference in input-frame units. These are observations from callbacks, not latency measurements. The current inventory supplies only system-reported nominal rate and channels. Actual stream format and buffer size, all queue/drift metrics, a real ten-minute run and observed latency remain **NOT VERIFIED**. No end-to-end Discord route has been tested.

## Validation still required

On a Mac where the microphone, BlackHole and a distinct physical output are visible to CPAL:

1. Run `cargo run --example mic_route_probe --locked -- list` from `src-tauri` and record exact names, IDs and device types. Select BlackHole as Discord's input; do not change the system default microphone.
2. Run `start --virtual "<BlackHole name>" --monitor "<physical name>" --mode both --seconds 600`, redirecting stdout to a local measurement log outside the repository. Record all device configurations and the ten-minute occupancy, overflow, underflow, discard and clock-drift trends. Do not estimate latency from buffer sizes.
3. In separate short runs, check microphone only, clip only and combined audio in Discord; verify that local monitoring contains only the clip and that Discord receives no further microphone audio after stopping capture. Test a missing or identical monitor device and a device failure.

As of 2026-10-26, those manual scenarios, a real 10-minute measurement, actual observed latency, Windows 11 and Intel macOS are **NOT VERIFIED**. Do not install a driver or change system defaults as part of device inventory. After the owner installs a suitable virtual device, refresh the inventory and record its configuration before running capture.

## Production integration path and risks

After owner acceptance of this ADR and successful real-device measurements, add an input and selected-output port beside the existing default-output engine, keeping the callback rules of ADR 0006. Define typed device selection and state/events, explicit capture start/stop and permission UX, and a separate approved settings migration only if device choices need persistence. Test stream recovery, drift and feedback on Windows and both Mac architectures before calling the route reliable. The current prototype's linear interpolation, 40 ms target and CPAL device classification are provisional; compare audible quality and queue trends before choosing production resampling and monitoring policy. No first-party virtual driver is proposed.
