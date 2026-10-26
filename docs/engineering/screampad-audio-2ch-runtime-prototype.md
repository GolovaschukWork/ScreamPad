# ScreamPad Audio 2ch local runtime prototype (2026-10-26)

This note records a **local, unsigned prototype** test under [ADR 0016](../architecture/decisions/0016-macos-integrated-audio-component.md). It is not a release qualification or a production-routing claim. The [release compatibility checklist](macos-audio-component-compatibility-checklist.md) remains open for a final signed/notarized package on supported architectures.

## Environment and artifact

- MacBook Pro, macOS 26.2 arm64, Xcode 26.2; original BlackHole 2ch `0.7.1` was already installed.
- Branded package: `ScreamPadAudio2ch-0.7.1.1-unsigned.pkg`, built by the [isolated recipe](../../packaging/macos/screampad-audio-2ch/README.md); SHA-256 `95a2f29012882a9b2c3acc90d358b2627bfd50d1308ffc1c15483b5490501a5d`.
- `spctl --assess --type install` rejected the package with `source=no usable signature`. `pkgutil --check-signature` reported no signature. The owner installed it manually in macOS Installer; the agent did not enter administrator credentials. CoreAudio did not enumerate it until the owner ran `sudo killall coreaudiod` in Terminal. The package has no installer scripts.
- Component bundle `/Library/Audio/Plug-Ins/HAL/ScreamPadAudio2ch.driver`, bundle ID `io.github.golovaschukwork.screampad.audio.driver`, receipt ID `io.github.golovaschukwork.screampad.audio.pkg.2ch`, version `0.7.1.1`.

## Static coexistence and enumeration

After restarting CoreAudio, `system_profiler SPAudioDataType` showed both BlackHole 2ch and ScreamPad Audio 2ch as separate virtual devices with two input and two output channels at 48 kHz. The read-only CPAL `mic_route_probe list` reported `coreaudio:BlackHole2ch_UID` and `coreaudio:io.github.golovaschukwork.screampad.audio.device.2ch` for both input and output roles. CoreAudio launched separate driver processes. The original BlackHole receipt remained `audio.existential.BlackHole2ch` version `0.7.1`; its executable SHA-256 remained `362b01eef3f6cb0e19f40f336a18c2cba3959a3c5f3c2311b022ba449007a1b7` before and after installation. MacBook Pro Microphone and Speakers remained the system default input/output.

The branded CPAL output reported F32, 48 kHz, two channels, supported buffer range 15–4096 frames, and an observed 512-frame callback buffer. The built-in microphone reported F32, 48 kHz, one channel, and an observed 512-frame input buffer in the combined test. These are device/stream observations, not latency measurements.

## Routing observations

| Scenario | Observation | Limit |
| --- | --- | --- |
| Clip-only into branded output, captured from its virtual input with FFmpeg AVFoundation | 15.984-second stereo 48 kHz recording contained nonzero test-tone samples: 16-bit PCM peak 4098, RMS 2066.44. A 4.424-second recording after stopping the probe was all zero. | This proves local loopback for the test clip, not call-app delivery. |
| First Discord Mic Test with built-in mic plus test tone | Owner heard nothing. | Tone/voice processing and Discord device state were not isolated in this run. |
| Discord Mic Test with clip-only synthesized speech, ScreamPad Audio 2ch selected as input, Noise Suppression and Echo Cancellation disabled | Owner heard the repeated spoken phrase. | Local Discord Mic Test only; no remote participant. |
| Discord Mic Test with built-in mic plus quieter synthesized speech | Owner initially heard both, then clarified that microphone voice was audible only while the phrase played. The explicit 25-second probe stopped and dropped all streams. | **FAILED** as a usable combined route in that run. Clip level, Discord processing and the virtual-input signal during clip pauses need separate diagnosis; no remote participant. |
| Discord Mic Test with built-in mic only, no clip | Owner heard microphone voice continuously. FFmpeg read the branded virtual input to a null output without saving audio: mean level −39.6 dB, peak −6.3 dB over 23 seconds. The 20-second probe then stopped cleanly. | Confirms mic-only delivery in local Mic Test, not reliable combined delivery or a remote recipient. |
| Combined mic plus very quiet synthesized speech with four-second pauses | Owner again heard microphone voice only during phrase segments in Discord Mic Test. In a separate 16-second run while the owner spoke continuously, FFmpeg sent branded-input audio to null and printed one-second RMS levels only. Eleven consecutive seconds showed about −37 to −45 dB, spanning at least one four-second clip-silence interval. No audio file was saved. | This supports microphone presence at the virtual input during clip pauses. Discord input sensitivity/processing is a plausible cause of the audible gating, but it has not been isolated by a controlled Discord-setting test. |
| Discord Mic Test after the combined probe stopped, branded input still selected | Owner heard no microphone voice. | Confirms stop behavior in this local test. |
| Explicit MacBook Pro Speakers local monitoring during clip-only routing | Both streams opened and callbacks ran, then the probe received a device-error flag after about one second and stopped all streams. | **FAILED / NOT VERIFIED**. CPAL reports built-in speaker type as `Unknown`; the experimental ID-based monitor allowance was removed from the probe. |

The 25-second combined run reported queue capacity 9,600 mono frames, maximum occupancy 3,584 frames, and zero overflow, underflow, or discarded frames. Both input and virtual output callbacks reached 512 frames. Reported clock-frame difference fluctuated up to about 1,536 frames over this short run; this does **not** establish long-term clock stability or end-to-end latency. No latency measurement was made.

The Mac's system input volume was 29 during diagnosis; it was not changed. [Discord's Voice Input Modes guide](https://support.discord.com/hc/en-us/articles/211376518-Voice-Input-Modes-101-Push-to-Talk-Voice-Activated) describes automatic and manual input sensitivity and warns that automatic sensitivity can cut off softer voice with large level variation. This is a hypothesis for the observed combined-test gating, not a verified root cause.

## Isolated level-control follow-up

The standalone `mic_route_probe` now accepts bounded `--mic-gain-db` (−12 to +12 dB) and `--clip-gain-db` (−36 to 0 dB). Both default to 0 dB to preserve previous output. The gains are parsed before streams start and converted to linear scalars outside callbacks. Callback work remains bounded and records source/output energy, output peak and clipped-frame count through atomics; the control loop prints derived RMS levels at the end of the run. These meters do not save voice samples. A clip-only run on the branded device reported output RMS −21.1 dBFS and peak −18.1 dBFS at 0 dB clip gain; at −12 dB clip gain it reported RMS −33.1 dBFS and peak −30.1 dBFS. Both runs had zero clipped frames and stopped cleanly. Seventeen example tests passed, including microphone continuity through a silent clip frame, gain validation, empty-clip safety and mix clipping. **No combined Discord test with adjusted gains has been performed**, so this change is an instrumented prototype, not a demonstrated fix for Discord gating.

## Remaining gates

- **NOT VERIFIED:** signed/notarized artifact, macOS 14 runtime, Intel runtime, fresh installation on a clean Mac, upgrade/reinstall/repair/interrupted install, removal/rollback, long-duration drift, and local monitoring isolation.
- **NOT VERIFIED:** a remote Discord/Meet participant receiving the mix. Reliable microphone delivery during combined playback remains **NOT VERIFIED** after the owner's correction; mic-only delivery was observed. The owner's Discord Mic Test observations must not be labeled `Discord Verified` in the app.
- No production audio engine, UI, IPC, SQLite, Tauri capability, or system default was changed. The unsigned branded component remains installed on this development Mac; do not distribute it or include it in a public ScreamPad installer.
