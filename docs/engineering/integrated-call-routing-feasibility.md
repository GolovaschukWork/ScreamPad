# Integrated call routing: platform feasibility notes

- Date: 2026-10-26
- Scope: read-only source review and macOS prototype evidence for Proposed ADR 0015
- Status: research, not a production design approval

## macOS: installing a virtual audio device

Apple's Audio Server plug-in sample publishes a virtual two-channel input/output device. Its documented installation places a `.driver` bundle in `/Library/Audio/Plug-Ins/HAL` and requires a reboot before inspection in Audio MIDI Setup. A ScreamPad-owned plug-in is technically plausible, but no ScreamPad plug-in has been built, installed, signed, notarized or tested. The sample is a starting point, not a ready routing implementation.

BlackHole is already installed on the development Mac and works as a loopback endpoint for the isolated probe. Its publisher says GPL-3.0 applications may integrate BlackHole; other applications need a separate license. The owner selected GPL-3.0-only for ScreamPad on 2026-10-26, resolving the earlier mismatch between the repository's GPLv3 `LICENSE` file and its TBD documentation. This makes BlackHole a distribution candidate under the publisher's stated guidance, subject to a recorded license and packaging compliance review. Using an owner-installed BlackHole as a test device does not itself validate a ScreamPad installer.

Apple describes distinct Developer ID Application and Developer ID Installer certificates, and notarization for software distributed outside the Mac App Store. The release policy also requires signing and notarization. A driver setup flow must account for installer consent, elevated installation, OS prompts, restart, device refresh, repair, upgrade, rollback and uninstall. None has been tested for ScreamPad. Do not silently run `sudo`, restart CoreAudio or change system default devices.

**Next macOS experiment:** compare a consent-based BlackHole package integration with a first-party Audio Server plug-in prototype built without installing it. Inspect input/output identity, packaging, signing, source/notice obligations and protocol. Installing a prototype into `/Library` requires a separately reviewed plan and explicit owner consent. Only then choose the component for production onboarding.

## Windows 11: processing the existing microphone

Soundpad documents that it configures the existing default recording device and that call apps using the same device often need no input change. This establishes a desirable user experience, not ScreamPad's implementation mechanism. Soundpad is Windows-only and proprietary; no Soundpad code or binaries are a prototype input.

Microsoft documents Audio Processing Objects (APOs) as user-mode system effects in the Windows audio engine. A custom APO is registered through a driver package and associated with an audio endpoint. Current componentized registration is device-relative: Microsoft's documentation says an APO cannot be registered once globally for multiple unrelated drivers. Extension INFs are targeted to device identifiers. **Inference:** an APO for arbitrary built-in, USB and Bluetooth microphones is not a universal one-click attachment mechanism without device-specific packaging and compatibility work. A Windows 11 hardware experiment is required before choosing it.

Processing mode also matters. Applications can request Raw mode. Microsoft says endpoint effects apply even to Raw streams, while other processing positions and app/driver behavior differ. This does not prove a ScreamPad clip-injection APO would reach Discord or Meet, or that a generic third-party endpoint effect can be installed on every microphone. It makes a representative device and app matrix essential.

Windows driver packages and APOs need an appropriate signing and deployment path. Attestation signing and WHQL have different implications; a lab build in test-signing mode is not a releasable installer. No Windows hardware or signing environment was available during this review, so Windows behavior is **NOT VERIFIED**.

**Next Windows experiment:** on Windows 11 x64 with the Windows Driver Kit, build an isolated Microsoft-documented APO/driver-extension sample for one explicitly chosen test microphone. Without changing its default-device status, verify pass-through when ScreamPad is off and a generated clip mixed with voice when on. Repeat with built-in, USB and Bluetooth devices and test Discord and Meet with normal and bypass/raw processing. Record failures and install/uninstall effects. If device-independent deployment is not viable, evaluate the separate signed virtual-microphone fallback from ADR 0015.

## Decision gates

| Gate | Evidence needed before production work |
| --- | --- |
| macOS component | Distribution rights or first-party implementation, signed/notarized install design, uninstall/rollback, actual device discovery and loopback |
| Windows existing-mic path | At least built-in, USB and Bluetooth compatibility; verified call-app mic-only, clip-only and combined scenarios; raw/bypass behavior; device-targeting and signed installation plan |
| User claim | A named app, OS, hardware, version and date with actual recipient-side evidence; local loopback or open streams alone are insufficient |

Local playback remains independent of every gate. No production audio, IPC, UI, installer, permissions or system defaults changed during this source review.

## Primary sources

- [Apple: Creating an Audio Server Driver Plug-in](https://developer.apple.com/documentation/coreaudio/creating-an-audio-server-driver-plug-in)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple: Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)
- [BlackHole publisher repository and licensing guidance](https://github.com/ExistentialAudio/BlackHole)
- [Soundpad: About](https://www.leppsoft.com/soundpad/about/)
- [Soundpad: Device setup](https://www.leppsoft.com/soundpad/en/help/manual/config/)
- [Microsoft: Implementing Audio Processing Objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/implementing-audio-processing-objects)
- [Microsoft: Audio Processing Object architecture](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-processing-object-architecture)
- [Microsoft: Audio signal processing modes](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-signal-processing-modes)
- [Microsoft: Extension INF targeting](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/submit-dashboard-extension-inf-files)
- [Microsoft: Driver signing options](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/driver-signing-offerings)
