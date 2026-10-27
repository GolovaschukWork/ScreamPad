# macOS production call-route validation (2026-10-26)

This record covers the production CPAL route in the `feat/macos-call-routing` feature branch. It is not a Discord recipient test or a release approval.

## Follow-up: silent microphone input (2026-10-27)

In the production UI, the owner observed about 2.05 million captured and rendered frames with zero queue underruns while the microphone and virtual-output levels remained at the meter floor during speech. Playing a library sound raised the virtual-output level. The macOS Sound input meter moved for the selected MacBook Pro Microphone, and ScreamPad was enabled under Privacy & Security > Microphone. These observations narrow the failure to microphone capture by this app; they do not prove a mixer or Discord defect.

The app had `NSMicrophoneUsageDescription` but no macOS Audio Input entitlement. A Tauri bundle built without a signing identity had no entitlements in `codesign -d --entitlements`. The project now declares `com.apple.security.device.audio-input` in `src-tauri/Entitlements.plist`. An ad hoc test bundle built with `APPLE_SIGNING_IDENTITY=- pnpm tauri build --bundles app` contained that entitlement, bound the `dev.local.soundboard` identifier in its signature, and passed `codesign --verify --deep --strict`. The owner reported that microphone routing works and self-audio is audible in that signed bundle. The exact listening application, simultaneous clip/voice quality, and remote Discord delivery are **NOT VERIFIED**. The ad hoc signature is for local development only and is not a release signature.

## Hardware and component

- Host: macOS 26.2 arm64, MacBook Pro. The selected capture device was `MacBook Pro Microphone` (`coreaudio:BuiltInMicrophoneDevice`, 1 input channel). The selected route output was `ScreamPad Audio 2ch` (`coreaudio:io.github.golovaschukwork.screampad.audio.device.2ch`, 2 input and 2 output channels). The local output was `MacBook Pro Speakers` (`coreaudio:BuiltInSpeakerDevice`, 2 output channels). All reported a current 48,000 Hz sample rate in System Information.
- Original `BlackHole 2ch` (`coreaudio:BlackHole2ch_UID`) also enumerated with separate input/output roles. MacBook Pro Microphone remained the default system input; MacBook Pro Speakers remained the default system and output device.
- The owner's locally installed unsigned 0.7.1.1 ScreamPad Audio package passed `verify-prototype.py` for its owned payload, bundle/package IDs, device UIDs, attribution, and arm64/x86_64 slices. Package SHA-256: `95a2f29012882a9b2c3acc90d358b2627bfd50d1308ffc1c15483b5490501a5d`. The package is a development artifact and is not signed, notarized, or included in a public installer.

## Ten-minute production stream run

Run with explicit microphone and route-device IDs using the ignored `production_route_runs_for_ten_minutes_with_bounded_queues` test. Both streams opened at 48,000 Hz. CPAL reported 512-frame buffers for capture and route output. The microphone queue capacity was 9,600 mono frames (200 ms hard bound), with a 40 ms control target.

| Minute | Queue occupancy / maximum (frames) | Captured / rendered frames | Overflow / underflow / discarded / clipped | Raw input-output frame-count delta |
| --- | --- | --- | --- | --- |
| 1 | 1,633 / 2,144 | 2,880,000 / 2,878,976 | 0 / 0 / 0 / 0 | +1,024 |
| 2 | 1,663 / 2,174 | 5,760,512 / 5,759,488 | 0 / 0 / 0 / 0 | +1,024 |
| 3 | 1,665 / 2,176 | 8,641,024 / 8,639,488 | 0 / 0 / 0 / 0 | +1,536 |
| 4 | 1,665 / 2,176 | 11,521,024 / 11,520,000 | 0 / 0 / 0 / 0 | +1,024 |
| 5 | 1,665 / 2,176 | 14,401,536 / 14,400,000 | 0 / 0 / 0 / 0 | +1,536 |
| 6 | 1,665 / 2,176 | 17,281,536 / 17,280,512 | 0 / 0 / 0 / 0 | +1,024 |
| 7 | 1,665 / 2,176 | 20,162,048 / 20,160,512 | 0 / 0 / 0 / 0 | +1,536 |
| 8 | 1,665 / 2,176 | 23,042,048 / 23,041,024 | 0 / 0 / 0 / 0 | +1,024 |
| 9 | 1,665 / 2,176 | 25,922,048 / 25,921,024 | 0 / 0 / 0 / 0 | +1,024 |
| 10 | 1,665 / 2,176 | 28,802,560 / 28,801,536 | 0 / 0 / 0 / 0 | +1,024 |

The run passed and Stop Routing closed capture. Occupancy stayed bounded without a rising trend. The frame-count delta samples are affected by asynchronous snapshots and are **not** a direct measurement of physical clock drift or end-to-end latency. Observed latency is **NOT VERIFIED**. A preceding run stopped at minute 6 with `NoDevice` after the engine required only one failed CoreAudio enumeration. The engine now requires three consecutive enumeration misses while still stopping immediately on actual stream failure; the passing run exercised that change. The exact cause of the preceding miss was not independently identified. A later change counts CPAL `Xrun` events without treating them as device loss; the ten-minute binary preceded that change.

## Imported-library fan-out

The ignored `imported_library_clip_reaches_local_and_virtual_callbacks` test imported the repository's `tone.mp3` through the real library service, started the production route, and pressed the equivalent of Library Play. Over 500 ms the local mixer ran 49 callbacks; the virtual mixer rendered 25,088 frames, reported nonzero clip and microphone energy, and recorded zero clip errors. Stop Sound and Stop Routing completed; local output remained running. The callback meters establish in-process signal delivery, not audible monitoring or Discord delivery. No microphone audio was saved.

## Remaining verification

- Production ScreamPad UI interaction, microphone permission prompt, local headphone/speaker audibility and absence of microphone monitoring: **NOT VERIFIED**. Computer-use automation access to the ScreamPad app was rejected by automatic approval review, so these require owner observation.
- Production route in Discord Mic Test, including mic-only, clip-only, combined, and stop: **NOT VERIFIED**. Prior isolated-probe Discord results do not substitute for this test.
- A real Discord call recipient hearing both voice and a library clip, other chat applications, macOS 14, Intel macOS, Windows, signed/notarized installation, clean-machine component lifecycle, and measured end-to-end latency: **NOT VERIFIED**.
