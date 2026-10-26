# ScreamPad Audio 2ch compatibility checklist

Complete this checklist **for each release candidate** using the final signed/notarized package on clean macOS 14+ arm64 and x64 systems. Record OS/build, hardware, package version/SHA-256, date, and evidence links. An empty box is **NOT VERIFIED**; a successful build is not runtime evidence. See [ADR 0016](../architecture/decisions/0016-macos-integrated-audio-component.md).

| Check | arm64 | x64 | Evidence to record |
| --- | --- | --- | --- |
| Payload, bundle/package/receipt IDs, all device/box/model/mirror UIDs and factory UUID match ADR; no original BlackHole path or ID claimed | [ ] | [ ] | Built plist, binary/string and package inspection |
| Developer ID Application/Installer signatures, notarization, stapling, Gatekeeper and payload checks pass | [ ] | [ ] | Commands, team ID, ticket, artifact hash |
| GPL notice, modifications, exact corresponding source and clean build recipe reachable for exact version | [ ] | [ ] | Versioned source URL, archive hash, build log |
| Fresh explicit install completes; CoreAudio enumerates two-channel input/output with supported configurations after Refresh/restart | [ ] | [ ] | Installer log, receipt, CPAL/CoreAudio IDs, formats |
| Original BlackHole 2ch and ScreamPad Audio coexist with distinct IDs/receipts and independent routes | [ ] | [ ] | Before/after inventory and routing test |
| Other compatible provider stays selectable; system defaults unchanged; local playback works without branded component | [ ] | [ ] | Selection/default snapshots, playback result |
| Mic-only, clip-only, combined route work; local monitor gets clip only; stop ends microphone delivery | [ ] | [ ] | Device configurations and remote-recipient result |
| Same-version reinstall and upgrade affect only owned files/receipt and reconcile selection | [ ] | [ ] | Before/after payload and receipt inventory |
| Missing/corrupt bundle and interrupted install show honest state; explicit repair restores enumeration without touching others | [ ] | [ ] | Failure injection, logs, recovery |
| Explicit unused uninstall removes only owned component; in-use/uncertain removal defers; normal app update leaves component | [ ] | [ ] | Other-app use, file/receipt inventory, app update |
| Failed upgrade explicitly rolls back to prior signed package without altering other providers | [ ] | [ ] | Rollback versions, enumeration, routing |

**Release decision:** Driver inclusion in a public installer/updater is blocked until every required check has evidence and the owner separately approves distribution. Discord/Meet verification requires an actual remote recipient; detection or a local CPAL stream is insufficient.
