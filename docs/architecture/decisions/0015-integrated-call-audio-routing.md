# ADR 0015: Integrated Call Audio Routing

- Status: Proposed
- Date: 2026-10-26
- Owner: project owner

## Context

The owner wants a user to install ScreamPad, add a sound, click it, and have people in a call hear it without separately downloading BlackHole or repeatedly choosing an input device. Local playback must remain usable without any call-routing component.

ADR 0003 chose user-installed external virtual devices for MVP. ADR 0014 implemented provider-neutral discovery and manual macOS setup; it expressly excludes installing a driver in ScreamPad. This proposal changes that product direction only if accepted. Until then, both Accepted ADRs govern production code.

The isolated macOS prototype in ADR 0013 sent microphone and test clip to BlackHole. A BlackHole input loopback observed the clip, but the owner's Discord check heard only the microphone. End-to-end call routing is not established by that measurement. Soundpad's publisher describes a different Windows experience: its installer configures the existing default recording device, so an app already using that device often needs no input change. Soundpad is Windows-only; its implementation is proprietary and is not a source of ScreamPad code.

## Options

| Option | User experience | Main cost or limitation |
| --- | --- | --- |
| Keep external virtual devices and manual setup on both platforms | Separate provider installation and per-app input selection | Lowest implementation cost; does not meet the owner's onboarding goal |
| Bundle a licensed virtual device on both platforms | One consent-based installer; choose its input once in each call app | Driver distribution, signing, upgrades, removal, support, and third-party licensing |
| macOS integrated virtual device; Windows existing-microphone audio processing | Guided install on Mac; existing microphone remains the call input on compatible Windows systems | Two platform-specific components; Windows compatibility and bypass modes require a spike |
| Per-app integrations or bots | Potentially little OS setup in supported apps | Cannot promise the same behavior across arbitrary desktop voice apps and meetings |

## Proposed direction

Prefer the third option, subject to the platform feasibility gates below. Reproduce Soundpad's **user-visible behavior** on Windows, not its code or an unverified claim about its internal driver architecture.

### Shared behavior

- Local library playback remains fully functional without a call-routing component, administrator rights, microphone permission, or a selected virtual device.
- The ScreamPad application opens its own microphone capture stream only after an explicit user action that starts call routing. Stop and release that stream on user stop, device failure, or app shutdown. A future Windows processing component may receive samples when a call app captures the configured microphone; while ScreamPad routing is off it must pass them through without adding clips. Its data access and lifecycle need a separate privacy review. Never change the system default input or output.
- Keep the audio mixer, devices, and routing state in Rust. Real-time callbacks follow ADR 0006: bounded buffers, no blocking or allocation, explicit overflow/underflow policy, independently clocked streams, and clean failure shutdown.
- Show separate states for component installed, device detected, routing active, and verified in a named call app. None implies the next. Do not promise that every call app will pass clips through its voice processing.
- Keep provider-neutral macOS discovery and manual setup as a fallback. Do not block Play or Stop in the library when call routing is unavailable.

### macOS path

Provide an in-app, consent-based setup flow for a compatible virtual audio device. Prefer a properly licensed, signed and notarized distributable component if one is available. If distribution rights or operational requirements make that unsuitable, evaluate a first-party Core Audio Audio Server plug-in. BlackHole remains an optional compatible provider, not a hardcoded requirement. Do not bundle or repackage BlackHole without written distribution rights for ScreamPad's license model.

The setup UI must explain the OS installer/admin and restart or CoreAudio-restart step before the user accepts it. The application must not silently install, elevate, change system defaults, or alter another app's input selection. After installation, refresh discovery, verify the input/output roles and supported configuration, select the route explicitly, and provide a short call-app test. A user may need to select the corresponding virtual input once in Discord, Meet, or another app. The existing manual provider-neutral path remains available.

The installer mechanism, package signing identity, updates, rollback/uninstall and licensing are **not approved by this proposal**. They require a reviewed implementation plan and owner approval before production changes.

### Windows path

First prototype a Windows 11 x64 component associated with an explicitly chosen *existing* recording device. Investigate supported Windows audio processing and driver extension APIs, including Audio Processing Objects (APOs), to determine whether ScreamPad can mix clips into the capture signal that ordinary call apps receive while preserving the physical microphone's device identity. Do not assume an APO works for every microphone, USB/Bluetooth driver, processing mode or app that bypasses system input processing. The precise mechanism must be selected from measured compatibility, not inferred from Soundpad's behavior.

If this path is feasible, the installer should offer the component only with explicit consent and any required Windows privilege prompt; setup should target the user's chosen recording device without changing the default. Provide a compatibility test, repair, disable and uninstall path. If the device or call app bypasses the processing component, show an unsupported state and offer a reviewed virtual-microphone fallback rather than claiming that routing is active.

The fallback would expose a separate input device and require one-time selection in each call app. It is not approved for production by this ADR. Both Windows approaches require a signed, distributable package and actual Windows 11 testing. No Soundpad binaries, source code, branding, assets or private protocols may be copied.

## Staged implementation and approval gates

1. **Feasibility and legal review.** Confirm macOS distribution rights or first-party plug-in scope; inspect current Apple install/signing requirements. Build an isolated Windows 11 prototype using documented Microsoft APIs and test built-in, USB and Bluetooth recording devices. Record whether call apps receive both mic and representative clips. No production installer or permissions change in this stage.
2. **Platform architecture decision.** Choose the exact macOS component and Windows injection or fallback mechanism from evidence. Review licensing, signing, privilege, uninstall, device-change and failure recovery. Update or supersede ADR 0003 and the installation exclusion in ADR 0014 only after owner approval.
3. **Production routing.** Integrate typed IPC, explicit capture controls, bounded Rust mixing, installer state, accessible EN/UK onboarding and telemetry-free diagnostics in separate reviewable PRs. Any new dependency, Tauri capability, system permission or persistent setting needs its own reviewed scope. Preserve the current local playback path.
4. **Release validation.** Test signed installers, fresh install, upgrade, uninstall and rollback on supported OS versions and architectures. Test Discord and Meet separately with microphone-only, clip-only and combined audio, voice processing settings, hotplug, sleep/wake, device failure, and stop-capture behavior. Record OS, app, component and device versions for every verified claim.

## Consequences and risks

- The improved onboarding shifts substantial work into system audio integration, distribution and support. A visible consent and repair flow is part of the product, not an optional polish step.
- Windows processing on the existing microphone could eliminate per-app input selection when the call app already uses that microphone, but its reliability and compatibility are **NOT VERIFIED**. A virtual-input fallback may still be necessary.
- On macOS, installing a virtual component inside a guided ScreamPad flow removes a separate download, but does not guarantee zero OS prompts or zero configuration in every call app.
- Audio filters, push-to-talk, input processing bypass, device drivers and independent clocks can prevent a stream that ScreamPad produced from reaching participants. A CPAL stream or local loopback test does not prove an end-to-end call route.
- The current macOS prototype is BlackHole-specific; production routing must use selected device IDs and validated roles/configurations rather than provider names.

## Validation status

- macOS arm64: BlackHole input loopback with the generated clip and a 600-second bounded-queue prototype run were observed; Discord received the microphone but the test tone was not heard. Combined Discord routing, local monitoring and end-to-end latency are **NOT VERIFIED**.
- macOS packaged-driver installation, Intel macOS, Windows 11 audio processing, signed distribution, Discord and Meet routing through a Windows component are **NOT VERIFIED**.
- This ADR changes documentation only. No production routing, installer, driver, permissions, dependencies or system defaults are changed by it.

## Sources

- [Apple: Creating an Audio Server Driver Plug-in](https://developer.apple.com/documentation/coreaudio/creating-an-audio-server-driver-plug-in)
- [Microsoft: Audio Processing Object architecture](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-processing-object-architecture)
- [Microsoft: Implementing Audio Processing Objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/implementing-audio-processing-objects)
- [Microsoft: Driver signing options](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/driver-signing-offerings)
- [Soundpad: About](https://www.leppsoft.com/soundpad/about/)
- [Soundpad: Device setup](https://www.leppsoft.com/soundpad/en/help/manual/config/)
- [Soundpad: Other operating systems](https://www.leppsoft.com/soundpad/en/help/faq/other-os)
- [BlackHole: Repository and licensing guidance](https://github.com/ExistentialAudio/BlackHole)
