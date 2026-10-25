# ADR 0014: Virtual Audio Device Onboarding

- Status: Proposed
- Date: 2026-10-26
- Owner: project owner

## Context

ADR 0003 chooses user-installed virtual audio devices for chat routing, while ordinary library playback must work without one. The production `AudioBackend` currently opens only the system-default output; `CpalBackend` does not yet enumerate devices for the UI. ADR 0013 is an isolated BlackHole prototype, not a provider policy or evidence of working Discord routing. On the development Mac, CPAL enumerated the built-in microphone and speakers but no BlackHole device; their reported `DeviceType` was `Unknown`. Device metadata alone therefore cannot establish whether a device is physical or virtual.

ScreamPad needs a setup path that explains what is missing without blocking local playback, installing software, changing the system microphone, or claiming that an enumerated device works in a chat application.

## Options

- **A: optional, provider-neutral discovery with guided setup** (proposed). Enumerate through the existing Rust audio infrastructure, show candidates and an explicit refresh action, and offer BlackHole as one official macOS installation option. Keep local playback independent.
- **B: require BlackHole before opening the library.** Rejected: it blocks the soundboard's local use and excludes other compatible virtual devices.
- **C: install or configure a driver automatically.** Rejected: it crosses OS privilege, third-party license and user-consent boundaries and is unnecessary for discovery.

## Proposed architecture

1. Add a non-real-time device-discovery operation to the audio backend port and its CPAL adapter, then expose a typed application/IPC query. Enumerate current macOS output devices with stable IDs where available, display names, reported device type, channel count, sample-rate/configuration support, and any paired input information the API exposes. Enumeration runs off audio callbacks and never changes the system default input or output. This ADR does not approve the future IPC change or any permission change.
2. Treat an output reported as virtual and compatible with the intended routing format as an **automatic candidate**. Device metadata may be incomplete: list unclassified outputs under a separate manual-selection path, with their type explicitly marked unknown. Do not infer a provider from a name, require the name "BlackHole", or equate a matching input/output ID with a successful end-to-end route. A physical output must not be silently selected as the chat route.
3. On opening Audio Routing, run one discovery request. **Refresh Devices** starts a new enumeration and replaces the previous result; ignore an older response that finishes later. If the selected device disappears, show it as unavailable and stop or refuse routing through it, while local playback continues. Distinguish no candidates from enumeration failure and from an unsupported device configuration.
4. In the first implementation, keep the selected virtual device in session memory. Persisting it would require a separately approved settings migration and a policy for stale device IDs. The local playback path continues to use the existing output engine independently of routing state.
5. Device presence and route health are separate facts: **Detected** means CPAL enumerated a candidate; **Selected** means the user chose it; **Routing active** would mean the future backend opened the route. None means Discord or another chat app received audio. An end-to-end **Verified** claim requires a recorded test with the named target app, OS, device and date; the UI must not manufacture a verified badge from discovery or stream creation.

## Proposed UX in Settings > Audio Routing

The page remains usable when no virtual device exists. Local library playback and its current audio status remain visible and functional; a routing setup card never covers or disables Play or Stop controls.

| Routing state | Primary message and action |
| --- | --- |
| Scanning | "Looking for audio devices…" with a non-blocking status indicator. |
| Setup required | "No virtual audio device detected. Local playback still works." Show **Get BlackHole for macOS**, **View setup steps**, and **Refresh Devices**. |
| Candidate detected | List detected virtual outputs by name with a **Detected · not tested in chat** label. Allow explicit selection; keep **Refresh Devices** available. |
| Unclassified outputs | In a separate "Other outputs" disclosure, explain that macOS did not identify the device type; require explicit user selection and show **Device type unknown**. |
| Selected or active | Show the selected device and backend route state separately. Prompt the user to select the corresponding virtual input in their chat app and test it there. Do not say "Discord ready". |
| Device missing or scan failed | Explain the specific problem; retain local playback and offer **Refresh Devices**. Do not relabel a scan failure as "no device installed". |

The setup steps are manual: open the [official BlackHole download page](https://existential.audio/blackhole/), follow the [publisher's installation guide](https://github.com/ExistentialAudio/BlackHole/wiki/Installation), complete any installer or restart prompts outside ScreamPad, return to Audio Routing, and choose **Refresh Devices**. Then select the virtual output in ScreamPad and its corresponding input in the chat application. ScreamPad does not install a driver, request administrator access, change the system default microphone, or change another application's settings. BlackHole is an example, not the only eligible provider. The official download flow can change, so link to the publisher's maintained page rather than a package file or a hardcoded version.

Use the existing `SettingsSection`/`SettingsRow`, Base UI controls, Tailwind semantic tokens, visible focus styles and Sonner feedback where appropriate. Text must use English and Ukrainian translation keys. The same layout and contrast requirements apply under Dark, Light and System themes, at the 640 × 480 minimum window size, with keyboard navigation and screen-reader status announcements. Setup links need descriptive accessible names and a clear indication that they open the publisher's site.

## Consequences and approval boundaries

- This proposal changes no production code, schema, dependencies, Tauri capability, CSP or installer behavior. Future typed IPC, external-link handling, microphone permissions, device selection and any persistent setting need their normal owner approvals before implementation.
- An external link must point to an approved fixed HTTPS URL; the UI must not turn device metadata into arbitrary URLs. ScreamPad does not bundle BlackHole. Review its publisher's license terms before any future distribution or integration decision.
- Virtual-device detection is best-effort. CPAL may report `Unknown`, devices can be unplugged or renamed, and a device can enumerate but fail to open or fail in Discord. A manual choice does not bypass format checks or turn an untested device into a verified one.
- Windows virtual-device onboarding needs its own platform evidence and provider guidance; this ADR proposes the macOS flow only.

## Validation for a later implementation

- Unit/component tests: local playback stays enabled in setup-required, scanning and error states; detected and verified wording differ; stale refresh responses are ignored; a missing selected device becomes unavailable; links use the fixed official URLs; keyboard and screen-reader states are exposed; EN/UK catalogs have parity.
- Backend tests: enumeration returns stable DTOs for virtual, physical and unknown device types; unsupported formats and enumeration errors are distinct; discovery never starts capture or changes system defaults.
- Manual macOS tests: installation outside the app followed by **Refresh Devices** updates the list; dark/light/system and narrow-window layouts work; a real chat call verifies microphone-only, clip-only and combined routing before any end-to-end claim. These behaviors, including Discord routing, are **NOT VERIFIED** by this proposal.

## Sources

- [BlackHole publisher download page](https://existential.audio/blackhole/)
- [BlackHole publisher installation instructions](https://github.com/ExistentialAudio/BlackHole/wiki/Installation)
- [BlackHole publisher repository and license notes](https://github.com/ExistentialAudio/BlackHole)
