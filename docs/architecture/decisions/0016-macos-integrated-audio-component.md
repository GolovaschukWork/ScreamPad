# ADR 0016: macOS Integrated Audio Component

- Status: Accepted
- Date: 2026-10-26
- Owner: project owner

## Context

ScreamPad's local soundboard must work without a virtual audio device. For macOS call routing, the owner wants one guided setup inside ScreamPad rather than a separate BlackHole download. ADR 0003 and ADR 0014 establish provider-neutral discovery and manual setup. This decision adds an **optional** ScreamPad-owned installation path on macOS and supersedes only their exclusion of a ScreamPad-installed driver. Existing providers, manual selection, and local playback remain supported. ADR 0015 governs the separate production-routing decision.

The owner selected GPL-3.0-only for ScreamPad. BlackHole's publisher says GPL-3.0 applications may integrate it, subject to distribution compliance. An isolated upstream BlackHole `v0.7.1` build and a temporary renamed build compiled on Xcode 26.2 for arm64 and x86_64. Neither branded build was signed, installed, or runtime-tested.

## Options

| Option | Benefit | Main risk or cost |
| --- | --- | --- |
| Keep only manual, provider-neutral setup | No ScreamPad-owned system component | Separate download and setup remain |
| Package unmodified BlackHole 2ch | Smallest source/build delta | Collides with an existing BlackHole package, device, and removal ownership |
| Package an attributed, ScreamPad-branded GPL build | Guided install with distinct ownership; manual alternatives remain | Maintain source, GPL delivery, signing, lifecycle, and compatibility |
| Build a first-party Audio Server plug-in | Full implementation control | Larger real-time and compatibility effort |

## Decision and production identity

Use a two-channel, ScreamPad-branded GPL build of BlackHole as an **optional macOS component**. Pin the upstream release and exact commit for each distributed version. `v0.7.1` is feasibility evidence, not a permanent version commitment. The production identity is independent of the app bundle ID and uses the project-controlled `io.github.golovaschukwork.screampad.audio` namespace. Changing any identity below requires an ADR amendment and migration review.

| Surface | ScreamPad production value | Original BlackHole 2ch in audited `v0.7.1` |
| --- | --- | --- |
| Plug-in `CFBundleIdentifier` and compiled `kPlugIn_BundleID` | `io.github.golovaschukwork.screampad.audio.driver` | `audio.existential.BlackHole2ch` in the 2ch package |
| Primary CoreAudio device UID | `io.github.golovaschukwork.screampad.audio.device.2ch` | `BlackHole2ch_UID` |
| Box UID | `io.github.golovaschukwork.screampad.audio.box.2ch` | `BlackHole2ch_UID` |
| Model UID | `io.github.golovaschukwork.screampad.audio.model.2ch` | `BlackHole2ch_ModelUID` |
| Hidden mirror UID, if retained | `io.github.golovaschukwork.screampad.audio.device.2ch.mirror` | `BlackHole2ch_2_UID` |
| Plug-in factory UUID | `DF08D44A-FEDF-5AC4-9796-F10DDC9A1970` | `E395C745-4EEA-4D94-BB92-46224221047C` in source plist; upstream packaging randomizes it |
| HAL bundle filename | `ScreamPadAudio2ch.driver` | `BlackHole2ch.driver` |
| Installer component package ID | `io.github.golovaschukwork.screampad.audio.pkg.2ch` | `audio.existential.BlackHole2ch` |
| Installer receipt identity | Same exact component package ID, queried with `pkgutil --pkg-info` | Original package ID above |
| Display name | `ScreamPad Audio 2ch` | `BlackHole 2ch` |

The macOS receipt is keyed by the component package ID; it cannot have a different independent identity. Set that ID explicitly in `pkgbuild --identifier` and match it in the distribution's `pkg-ref`; do not rely on ID inference from the bundle. Use `ScreamPadAudio2ch-<version>.pkg` for the outer file, but never infer ownership from its filename. Set the Xcode product/executable/`CFBundleName`, icon resource, both bundle-ID surfaces, all device/box/model/mirror UIDs, and factory UUID consistently. The factory UUID must remain stable across ScreamPad Audio upgrades. The standard Audio Server plug-in type UUID in `CFPlugInTypes` is an Apple interface identifier and stays unchanged. Do not present upstream's manufacturer as if it signed the branded component; retain BlackHole authorship in notices.

These proposed values do not collide with inspected upstream `v0.7.1` or the original BlackHole 2ch installed on the development Mac. This is a **static identity comparison**, not proof of CoreAudio coexistence or global uniqueness. Before release, inspect signed artifacts and installed receipts/devices for collisions, verify namespace and Apple signing-team ownership, and test both devices together. Upstream `kDevice_UID` and `kBox_UID` share a derived value, so build flags alone cannot implement the separate values above: a reviewed source patch is required. Audit hidden mirror visibility/UID even if disabled. Confirm two-channel input/output roles, supported formats, default-device eligibility, manufacturer string, and icon. Never change system defaults.

## Upstream installer and build-identifier audit

Audited official BlackHole `v0.7.1`, commit `e2b22aaaba4e507a097131704bf96dabc004d9cf`, under `Installer/`, `Uninstaller/`, `BlackHole/BlackHole.c`, `BlackHole/BlackHole.plist`, and `BlackHole.xcodeproj/project.pbxproj`. This is source inspection; no upstream install/uninstall script was executed or copied.

| Upstream file | Observed behavior or identifier | ScreamPad rule |
| --- | --- | --- |
| `Installer/create_installer.sh` | Loops over 2/16/64/128/256 channels; hardcodes `BlackHole`, `audio.existential.BlackHole*`, publisher signing team `Q5C99V536K` and notary profile; rewrites factory UUID; `pkgbuild` omits explicit `--identifier` while distribution uses `pkg-ref`; appends to `distribution.xml`; creates packages; removes `build`, `Installer/root`, and distribution files | Do not reuse. Write a scoped two-channel recipe with explicit package ID, owned staging paths, reviewed cleanup, and ScreamPad signing credentials outside the repo |
| `Installer/scripts/preinstall` | Creates shared HAL directory, changes its ownership and permissions | Do not copy broad shared-directory permission changes |
| `Installer/scripts/postinstall` | Recursively changes ownership and permissions through `/Library/Audio/Plug-Ins/HAL/BlackHole*ch.driver` | Do not copy wildcard behavior that touches other BlackHole variants |
| `Installer/requirements.xml` | macOS minimum 10.10 | Set and validate macOS 14+ target in new distribution metadata |
| `Installer/welcome.html`, `Installer/conclusion.html` | Publisher-specific naming, links, removal text, and restart message | Replace with accurate ScreamPad UX and attribution; review links/restart claim |
| `Uninstaller/create_uninstaller.sh` | Builds removers for all channel counts, rewrites postinstall, embeds publisher signing ID, removes generated script | Do not reuse; eventual removal targets only exact owned bundle/receipt |
| `Uninstaller/Scripts/postinstall` | Deletes `BlackHole256ch.driver` with `sudo rm -R`; restarts `coreaudiod` with `sudo killall` | Do not copy or invoke; this disrupts other audio clients |
| `BlackHole.c` | Defaults to BlackHole name, bundle ID, icon, manufacturer, device/box/model/mirror UIDs, two channels, hidden mirror, and default-device eligibility | Patch/review every shipped identity and behavior; preserve attribution separately |
| `BlackHole.plist`, Xcode project | Plist expands `PRODUCT_BUNDLE_IDENTIFIER`, `PRODUCT_NAME`, `EXECUTABLE_NAME`, versions, icon, and factory UUID; Xcode defaults to `audio.existential.BlackHole` and `BlackHole.driver` | Inspect built plist, binary strings, code signature, payload paths, and package receipt; source settings alone are insufficient |

No upstream installer or remover script may enter production without line-by-line side-effect review, exact-path scoping, idempotence checks, and interruption tests. Each upstream version bump requires a fresh scripts, identifiers, payload, and license audit.

## Installer and UX contract

1. Settings > Audio Routing offers **Set up ScreamPad Audio** only when absent or needing repair. Before launch, explain that it installs a system audio component, Installer may request administrator approval and restart, local playback does not need it, and a call app may need one-time virtual-input selection. Require an explicit action; declining is supported.
2. ScreamPad may open only its fixed, bundled, signed package through macOS Installer. A future typed backend command takes no path, URL, or shell text from frontend or device metadata. Verify fixed-resource identity and integrity before opening. ScreamPad itself must not run `sudo`, write `/Library`, restart CoreAudio, collect administrator credentials, or change default input/output.
3. **Installer Opened** is not **Installed**. On return/relaunch, **Refresh Devices** reconciles receipt/version, enumeration, input/output roles, supported configuration, and selected device ID. A receipt or detected device alone does not prove routing.
4. Retain `Scanning`, `Setup Required`, `Candidate Detected`, `Device Selected`, `Device Unavailable`, `Discovery Failed`, and `Unsupported Configuration`. Add `Consent`, `Installer Opened`, `Restart Required`, `Installed but Not Detected`, `Repair Available`, and `Install Failed` only with evidence. Never show **Routing Active** before production routing starts or **Discord Verified** without an end-to-end test.
5. Preserve provider-neutral discovery, manual selection of unknown-type devices, official provider links, EN/UK text, keyboard/screen-reader support, and Dark/Light/System themes. Do not auto-select a physical output. Library and local playback remain usable without any virtual device.

## GPL source and release provenance

For **each** distributed ScreamPad Audio version, publish and retain a manifest tying package version/SHA-256 to upstream tag/commit, ScreamPad patch set/commit, compiler/Xcode/macOS SDK versions, build flags, channel count, package recipe, and signing/notarization artifact IDs. Preserve upstream copyright notices and the GPLv3 license in package/app notices; label the binary as a modified BlackHole build, identify ScreamPad's modifications, and avoid implying upstream endorsement.

Make the **exact corresponding source** for each shipped driver available to recipients for the GPL-required period: pinned upstream source, all ScreamPad modifications, generated or maintained build/install scripts needed to produce the object code, and full recipe/toolchain requirements. Publish an immutable version-specific source archive or equivalent alongside each binary release, with stable access instructions in notices and Installer. Validate that clean source plus documented tools reproduces unsigned payload identities; signatures may differ. Retain old-version source after upgrades. Review GPL obligations for the combined app, bundled dependencies, and FFmpeg before distribution; publisher integration guidance does not replace that audit.

## Signing, notarization, and package verification

Sign the nested `.driver` bundle and Mach-O with the ScreamPad team's **Developer ID Application** certificate and the installer package with its **Developer ID Installer** certificate. Use hardened runtime/entitlements only as required by Apple's notarization rules and actual plug-in; review exceptions. Notarize the distributable package/app container with Apple's notary service and staple tickets where supported. Keep certificates, private keys, API credentials, and notary profiles outside the repo, PR CI, app resources, and logs. Never reuse publisher credentials embedded upstream.

Before release, inspect final payload with `pkgutil --expand`/payload listing and `plutil`; verify exact bundle/package IDs, receipt version, factory UUID, UID strings, architecture slices, install location, absence of broad-effect scripts, and no path outside owned bundle. Check `codesign --verify --strict --deep`, `codesign -dv`, `pkgutil --check-signature`, `xcrun stapler validate`, and applicable `spctl --assess` on final artifacts. Confirm notarization acceptance and clean-machine Gatekeeper/Installer behavior. Record hashes/logs without secrets. An unsigned local build is not release evidence.

## Lifecycle and compatibility

- **Original BlackHole plus ScreamPad Audio:** distinguish by stable IDs; never overwrite `BlackHole*` payloads/receipts or another provider's settings. Verify independent enumeration and routing after installation/restart.
- **Upgrade/reinstall:** compare owned receipt, signed bundle identity, and version. Replace only `ScreamPadAudio2ch.driver` through a signed package with stable IDs. Same-version reinstall is explicit; verify enumeration/routing afterward. Retain selection only while UID exists.
- **Repair:** when receipt, bundle, signature, and enumeration disagree, report observed state and offer explicit signed repair. Never treat receipt as proof CoreAudio loaded the driver or remove another provider.
- **Interrupted install:** keep local playback usable, report incomplete/unknown state, and re-check receipt, payload, signature, and enumeration on restart/Refresh. Do not auto-run Installer. Explicit repair must be idempotent.
- **Uninstall:** require a separately reviewed, explicit user action. Target only owned bundle/receipt; no wildcard or silent CoreAudio restart. If another app may use ScreamPad Audio, defer removal and ask to close it; inability to determine use is reason to defer. Clear selection only after confirmed removal. Never remove it automatically on a normal ScreamPad update or because ScreamPad no longer uses it.
- **Rollback:** retain prior signed/notarized package and exact source. After failed upgrade, guide an explicit reinstall of that owned version; re-check signature, receipt, enumeration, routing, and coexistence. Never roll back or remove another provider.

Use the separate [compatibility checklist](../../engineering/macos-audio-component-compatibility-checklist.md) as the per-version evidence record. macOS 14+ arm64 and x64 require coverage before public distribution. Windows is out of scope.

## Reviewable implementation sequence and release gate

1. **Build/package prototype:** pin source and patch set in isolated packaging area; audit identities and unsigned payload. This ADR PR adds no driver, installer, production routing, UI, IPC, or Tauri capability.
2. **Signing/compliance design:** review Developer ID team/certificates, source publication, signed contents, notarization, upgrade, repair, removal, and rollback. Privileged installation requires separate owner approval.
3. **Backend bridge and frontend onboarding:** separately review a fixed-resource, parameterless typed command, narrow capabilities, consent, refresh reconciliation, translations, accessibility, and themes. Discovery never starts microphone capture or affects local playback.
4. **Production routing:** separately approve under ADR 0015. Test mic-only, clip-only, combined, monitoring isolation, stop, device loss, drift, and remote Discord/Meet recipient. Detection or Installer success is not end-to-end proof.
5. **Public-distribution gate:** no branded driver in any public ScreamPad installer/updater until the **final signed/notarized package** passes installation, CoreAudio enumeration, original-BlackHole coexistence, routing, removal, and rollback on supported macOS architectures, with checklist and GPL source/recipe complete. Missing/failed results block inclusion. Release still needs separate owner approval.

## Validation status

- **Verified:** upstream and temporary branded unsigned `v0.7.1` builds on macOS 26.2 arm64/Xcode 26.2 produced universal arm64/x86_64 binaries. The local unsigned 0.7.1.1 package passed the payload identity verifier and was installed by the owner. CoreAudio enumerates `ScreamPad Audio 2ch` and the original `BlackHole 2ch` under distinct UIDs; the MacBook microphone and speakers remain system defaults. Earlier isolated probes passed owner-observed Discord Mic Test mic-only, clip-only, combined at reduced clip level, and stop-capture checks, with inconsistent voice audibility in some combined runs.
- **NOT VERIFIED:** signed/notarized package, privilege/restart UX on clean machines, upgrade/reinstall/repair/interruption/removal/rollback, Intel runtime, recipient-side Discord/Meet call, production app Mic Test, local monitoring, and observed latency.

## Implementation notes (production-routing feature PR)

The macOS app now extends the existing CPAL audio worker with an explicit microphone-to-selected-virtual-output route. Real library clips fan out to that route and the local output; microphone samples never enter the local mixer. Device choices live in session state. The app opens a user-selected local package in standard Installer only after a click. It does not bundle a driver, install it itself, elevate, or change system defaults. This development-only chooser does not satisfy the fixed signed-package contract above; replace it with a verified, bundled package before public distribution.

The local 0.7.1.1 unsigned package is a development artifact only. Developer ID Application/Installer signing, notarization, exact corresponding source publication, clean-machine install, upgrade, removal and rollback remain release gates. A production stream starting successfully does not establish Discord delivery; the combined production-app Mic Test and a remote-recipient call must be recorded separately. The [production-route validation record](../../engineering/macos-production-route-validation.md) reports the opt-in ten-minute CPAL run, actual configuration, queue bounds, drops, underflows, and clock delta without saving microphone audio.

The owner accepted this architectural direction on 2026-10-26 subject to the identity, audit, GPL, signing, lifecycle, and release-gate clarifications recorded here. Acceptance authorizes architecture and reviewable prototypes; it does **not** authorize production distribution, privileged installation, or automatic merge.

## Sources

- [BlackHole publisher repository, customization and GPL guidance](https://github.com/ExistentialAudio/BlackHole)
- [Apple: Creating an Audio Server Driver Plug-in](https://developer.apple.com/documentation/coreaudio/creating-an-audio-server-driver-plug-in)
- [Apple: Packaging Mac software for distribution](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [ScreamPad platform feasibility notes](../../engineering/integrated-call-routing-feasibility.md)
