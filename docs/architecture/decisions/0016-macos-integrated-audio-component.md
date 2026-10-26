# ADR 0016: macOS Integrated Audio Component

- Status: Proposed
- Date: 2026-10-26
- Owner: project owner

## Context

ScreamPad's local soundboard must work without a virtual audio device. For call routing on macOS, the owner wants one guided setup inside ScreamPad instead of a separate BlackHole download. ADR 0003 currently targets user-installed external devices, and Accepted ADR 0014 explicitly excludes driver installation. This record proposes a narrower change to those decisions; it does not change them while its status is Proposed.

The owner selected GPL-3.0-only for ScreamPad. BlackHole's publisher says GPL-3.0 applications may integrate it, subject to normal distribution compliance. An isolated BlackHole `v0.7.1` build succeeded on Xcode 26.2 for arm64 and x86_64. A second build with temporary ScreamPad naming and bundle identity also compiled. Neither was signed, installed or tested as a renamed device. Existing BlackHole 2ch worked in a macOS arm64 Discord Mic Test with voice and a file-backed clip after the clip's level was reduced; a remote participant has not verified a call.

## Options

| Option | Benefit | Main risk or cost |
| --- | --- | --- |
| Keep the manual, provider-neutral setup from ADR 0014 | No ScreamPad-owned system component | Separate download and setup remain |
| Package unmodified BlackHole 2ch | Smallest source/build delta | Shares package and device identity with user-installed BlackHole; upgrade and removal ownership are ambiguous |
| Package a clearly attributed, ScreamPad-branded GPL build of BlackHole | One guided install; separate identity and lifecycle; existing providers remain usable | Maintain a fork/build recipe and source obligations; signing, notarization, install and real-device behavior still need proof |
| Build a first-party Audio Server plug-in from scratch | Full control of implementation | Larger real-time, compatibility and maintenance effort before the routing UX is validated |

## Proposed decision

Use a two-channel, ScreamPad-branded GPL build of BlackHole as the **optional macOS component**. Pin an audited upstream release and source revision; the `v0.7.1` build is current feasibility evidence, not a permanent version commitment. Give the plug-in a unique production bundle ID, device UID and package ID owned by ScreamPad. Choose those IDs only after the publisher and production bundle identifier are settled. Use a user-facing name such as **ScreamPad Audio 2ch**, while retaining BlackHole attribution and license notices. Do not reuse the existing `BlackHole2ch_UID`, overwrite its `.driver` bundle, or uninstall another provider's device.

The current provider-neutral CPAL discovery remains available. If a compatible virtual device is already detected, let the user select it and skip installation. Unknown-type devices remain available for explicit manual selection after input/output role and configuration checks. Device names are display text, not stable identity; compare supported device IDs after Refresh. Do not select a physical output automatically. Local playback and the library remain fully usable when no component is installed, installation is declined, or installation fails.

## Installer and UX contract

1. Settings > Audio Routing offers **Set up ScreamPad Audio** only when the bundled component is absent or needs repair. Before launching anything, show that this installs a system audio component, macOS Installer may request administrator approval and a restart, local playback does not need it, and a call app may need a one-time selection of the virtual input. Require an explicit button press; declining is a complete, supported state.
2. The ScreamPad process launches only its fixed, bundled, signed installer package through the macOS Installer UI. The typed backend command takes **no path, URL or shell text** from the frontend or device metadata. Resolve the package from the app's fixed resource location and verify its expected identity/integrity before opening it. Do not run `sudo`, modify `/Library` directly from the app, restart CoreAudio, request administrator credentials in ScreamPad, or change system default input/output.
3. The OS Installer owns the privileged copy into `/Library/Audio/Plug-Ins/HAL` and any restart prompt. ScreamPad shows **Installation opened**, not **Installed**, when it launches Installer. On return or relaunch, **Refresh Devices** checks actual enumeration, input/output roles, supported stream configuration and the selected device ID. A package receipt alone cannot establish routing health.
4. Keep the existing `Scanning`, `Setup Required`, `Candidate Detected`, `Device Selected`, `Device Unavailable`, `Discovery Failed` and `Unsupported Configuration` states. Add installation-specific `Consent`, `Installer Opened`, `Restart Required`, `Installed but Not Detected`, `Repair Available` and `Install Failed` states only where their evidence is available. Never show **Routing Active** before production routing starts, or **Discord Verified** from installation or device detection.
5. Keep fixed official provider links and the manual instructions as an alternative. Use the existing Tailwind/Base UI design, Dark/Light/System themes, EN/UK text, keyboard navigation and screen-reader announcements. A setup card never disables local library playback.

## Build, security and distribution boundaries

- Keep the BlackHole source revision, build flags, package contents and artifact hashes reproducible and recorded. Include BlackHole copyright, GPL text, modifications and a way to obtain the corresponding source for the exact shipped binary. Audit ScreamPad's other bundled dependencies and FFmpeg configuration before distribution. Do not copy the publisher's signing identity or run its installer script unchanged.
- Sign the plug-in and installer with the correct ScreamPad Developer ID identities and validate Apple's notarization rules for the chosen app/DMG/package nesting. No signing credentials may enter PR CI, the repository, app resources or diagnostic output. A local unsigned build is only a development artifact.
- Package updates must target only ScreamPad's own bundle/package IDs. Before shipping, define and test receipt/version checks, interrupted-install recovery, repair, rollback and removal of **only** the ScreamPad-owned component. Do not silently uninstall a device while another app may be using it. Keep signed release and updater assets under the release policy's separate approval gate.
- The frontend receives only typed installation/discovery states. It cannot request an arbitrary local installer, process or URL. Avoid broad Tauri shell/opener permissions; describe any required fixed-resource and command capability change in its own review. The installation path never starts microphone capture.
- This proposal does not itself add BlackHole or any other dependency to ScreamPad. Production microphone routing, persistent device selection, microphone permission changes, SQLite migration and Windows behavior remain separate reviewed scopes under ADRs 0013–0015. The proposed BlackHole component still needs an approved implementation and compliance PR after this ADR is accepted.

## Reviewable implementation sequence

1. **Build/package prototype:** add a pinned source and build recipe in an isolated packaging area, plus a local unsigned package inspection test. Do not install, sign, notarize or add a package to a distributed app in this PR. Verify separate IDs and that the package payload cannot overwrite unmodified BlackHole.
2. **Signed distribution design:** define final IDs, source/notice publication, signing and notarization job, package verification, release artifact layout, upgrade and removal. Review the exact developer identities, CI secret boundary and rollback procedure before enabling distribution.
3. **Backend setup bridge:** add a macOS-only, parameterless typed IPC command to open the verified bundled package in Installer, plus read-only installation state. Keep command handling off audio callbacks. Add negative tests for missing/tampered resource and unsupported platform; do not use an arbitrary shell command or path from IPC.
4. **Frontend onboarding:** add explicit consent and progress/error states, fixed package action, refresh reconciliation, EN/UK accessibility tests and 640 × 480 Dark/Light/System checks. Device discovery remains provider-neutral and local playback independent.
5. **Routing and release validation:** after a separately approved production-routing plan, test microphone-only, clip-only and combined audio with a remote Discord and Meet participant; verify stop, gain, monitoring isolation, device loss and independent clocks. Test fresh install, existing BlackHole, upgrade, repair and removal on macOS 14+ arm64 and x64 with signed/notarized artifacts.

## Validation status and acceptance gate

- **Verified:** unsigned upstream and temporary branded `v0.7.1` builds on macOS 26.2 arm64/Xcode 26.2 produce universal arm64/x86_64 binaries. Existing BlackHole 2ch passed owner-observed Discord Mic Test mic-only, clip-only, combined at reduced clip level and stop-capture checks on this Mac.
- **NOT VERIFIED:** branded device enumeration or coexistence after installation, a signed/notarized package, privilege and restart UX, upgrade/removal, Intel macOS runtime, recipient-side Discord/Meet calls, local monitoring, end-to-end latency and any production routing.

Owner approval of this ADR and the exact system-component boundary is required before changing Accepted ADR 0003/0014 or implementing the installer/permissions. Approval to use GPL-3.0-only does not by itself approve a privileged installation path. No automatic merge or release follows from this proposal.

## Sources

- [BlackHole publisher repository, customization and GPL guidance](https://github.com/ExistentialAudio/BlackHole)
- [Apple: Creating an Audio Server Driver Plug-in](https://developer.apple.com/documentation/coreaudio/creating-an-audio-server-driver-plug-in)
- [Apple: Packaging Mac software for distribution](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [ScreamPad platform feasibility notes](../../engineering/integrated-call-routing-feasibility.md)
