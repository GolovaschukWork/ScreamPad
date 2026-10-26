# ADR 0015: Integrated Call Audio Routing

- Status: Proposed
- Date: 2026-10-26
- Owner: project owner

## Context

The owner wants a user to install ScreamPad, add a sound, click it, and have people in a call hear it without separately downloading BlackHole or repeatedly choosing an input device. Local playback must remain usable without any call-routing component.

ADR 0003 chose user-installed external virtual devices for MVP. ADR 0014 implemented provider-neutral discovery and manual macOS setup. Accepted ADR 0016 now authorizes the architecture for an optional ScreamPad-branded macOS component while keeping production distribution and privileged installation behind separate gates. This proposal still governs the broader cross-platform routing choice and remains Proposed.

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

Provide an in-app, consent-based setup flow for a compatible virtual audio device. ADR 0016 selects an optional, attributed ScreamPad-branded GPL build of BlackHole for macOS and defines its identity, compliance, signing, lifecycle and release gates. Existing BlackHole and other compatible providers remain manually selectable. A first-party Core Audio Audio Server plug-in remains a fallback if distribution or operational requirements cannot be met.

On 2026-10-26, the owner selected GPL-3.0-only for ScreamPad. BlackHole's publisher states that GPL-3.0 applications may integrate it. ADR 0016 selects a branded build as the architectural direction; it does not approve a public installer or complete distribution compliance.

The setup UI must explain the OS installer/admin and restart or CoreAudio-restart step before the user accepts it. The application must not silently install, elevate, change system defaults, or alter another app's input selection. After installation, refresh discovery, verify the input/output roles and supported configuration, select the route explicitly, and provide a short call-app test. A user may need to select the corresponding virtual input once in Discord, Meet, or another app. The existing manual provider-neutral path remains available.

The exact installer implementation, signing identity, updates, rollback/uninstall and version-specific GPL delivery remain separate review and release gates under ADR 0016. This proposal does not authorize production changes.

### Windows path

First prototype a Windows 11 x64 component associated with an explicitly chosen *existing* recording device. Investigate supported Windows audio processing and driver extension APIs, including Audio Processing Objects (APOs), to determine whether ScreamPad can mix clips into the capture signal that ordinary call apps receive while preserving the physical microphone's device identity. Do not assume an APO works for every microphone, USB/Bluetooth driver, processing mode or app that bypasses system input processing. The precise mechanism must be selected from measured compatibility, not inferred from Soundpad's behavior.

If this path is feasible, the installer should offer the component only with explicit consent and any required Windows privilege prompt; setup should target the user's chosen recording device without changing the default. Provide a compatibility test, repair, disable and uninstall path. If the device or call app bypasses the processing component, show an unsupported state and offer a reviewed virtual-microphone fallback rather than claiming that routing is active.

The fallback would expose a separate input device and require one-time selection in each call app. It is not approved for production by this ADR. Both Windows approaches require a signed, distributable package and actual Windows 11 testing. No Soundpad binaries, source code, branding, assets or private protocols may be copied.

## Staged implementation and approval gates

The [macOS production MVP integration plan](../../engineering/macos-call-routing-mvp-plan.md) identifies the current code seams, a narrow sequence of reviewable changes, explicit capture/IPC/permission boundaries, and release evidence. It is a proposal for the owner's architecture review, not authorization to implement production routing.

1. **Feasibility and legal review.** Complete the version-specific macOS distribution audit under ADR 0016. Build an isolated Windows 11 prototype using documented Microsoft APIs and test built-in, USB and Bluetooth recording devices. Record whether call apps receive both mic and representative clips. No production installer or permissions change in this stage.
2. **Platform architecture decision.** ADR 0016 selects the macOS component architecture and narrowly supersedes the installation exclusion in ADRs 0003/0014. Choose the Windows injection or fallback mechanism from evidence. Review licensing, signing, privilege, uninstall, device-change and failure recovery before production integration.
3. **Production routing.** Integrate typed IPC, explicit capture controls, bounded Rust mixing, installer state, accessible EN/UK onboarding and telemetry-free diagnostics in separate reviewable PRs. Any new dependency, Tauri capability, system permission or persistent setting needs its own reviewed scope. Preserve the current local playback path.
4. **Release validation.** Test signed installers, fresh install, upgrade, uninstall and rollback on supported OS versions and architectures. Test Discord and Meet separately with microphone-only, clip-only and combined audio, voice processing settings, hotplug, sleep/wake, device failure, and stop-capture behavior. Record OS, app, component and device versions for every verified claim.

## Consequences and risks

- The improved onboarding shifts substantial work into system audio integration, distribution and support. A visible consent and repair flow is part of the product, not an optional polish step.
- Windows processing on the existing microphone could eliminate per-app input selection when the call app already uses that microphone, but its reliability and compatibility are **NOT VERIFIED**. A virtual-input fallback may still be necessary.
- On macOS, installing a virtual component inside a guided ScreamPad flow removes a separate download, but does not guarantee zero OS prompts or zero configuration in every call app.
- Audio filters, push-to-talk, input processing bypass, device drivers and independent clocks can prevent a stream that ScreamPad produced from reaching participants. A CPAL stream or local loopback test does not prove an end-to-end call route.
- The isolated macOS probe accepts the original BlackHole 2ch and branded ScreamPad Audio 2ch by explicit device ID. Production routing must remain provider-neutral and use selected device IDs with validated roles/configurations rather than provider names.

## Validation status

- A dated [platform feasibility note](../../engineering/integrated-call-routing-feasibility.md) records source-backed macOS installation constraints and Windows APO device-targeting limits. It does not select a production component.
- macOS arm64: the [unsigned ScreamPad Audio 2ch runtime prototype](../../engineering/screampad-audio-2ch-runtime-prototype.md) enumerated alongside original BlackHole. Its virtual input carried a clip, and the owner heard clip-only speech in Discord Mic Test. Mic-only voice was continuously audible, but two combined runs had audible voice only during phrase segments, even with a quiet clip and long pauses. A separate level-only measurement showed signal at the virtual input during clip pauses; Discord processing is suspected but unproven. An earlier tone-based Mic Test heard nothing. Local monitoring failed after a device error. These observations do not verify a remote call recipient or release readiness.
- Signed/notarized distribution, macOS 14 and Intel runtime, upgrade/repair/removal/rollback, long-duration drift, local monitoring, remote Discord/Meet recipients, and Windows 11 audio processing are **NOT VERIFIED**.
- This Proposed ADR authorizes no production routing, installer, driver, permission, dependency or system-default change by itself.

## Sources

- [Apple: Creating an Audio Server Driver Plug-in](https://developer.apple.com/documentation/coreaudio/creating-an-audio-server-driver-plug-in)
- [Microsoft: Audio Processing Object architecture](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-processing-object-architecture)
- [Microsoft: Implementing Audio Processing Objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/implementing-audio-processing-objects)
- [Microsoft: Driver signing options](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/driver-signing-offerings)
- [Soundpad: About](https://www.leppsoft.com/soundpad/about/)
- [Soundpad: Device setup](https://www.leppsoft.com/soundpad/en/help/manual/config/)
- [Soundpad: Other operating systems](https://www.leppsoft.com/soundpad/en/help/faq/other-os)
- [BlackHole: Repository and licensing guidance](https://github.com/ExistentialAudio/BlackHole)
