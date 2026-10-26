# Virtual audio device onboarding implementation plan

ADR 0014 was accepted on 2026-10-26. This plan does not authorize production routing, persistent device selection, driver installation, microphone capture or changing system defaults.

## Change 1: read-only backend discovery

- Add a separate read-only `DeviceDiscovery` application port and CPAL adapter under the existing audio infrastructure. Create the CPAL host inside a blocking worker; do not move or modify the engine-owned `AudioBackend`. CPAL 0.18.2 is pinned in `src-tauri/Cargo.toml`; `HostTrait::input_devices`/`output_devices`, `DeviceTrait::id`/`description`/`supported_output_configs` and `DeviceId` are available. Keep `open_default_output` and local playback unchanged.
- Merge input/output roles by CPAL `DeviceId`, not display name. Return candidate type `virtual`, `physical` or `unknown` from best-effort metadata. Preserve unknown outputs for manual selection. Include only devices with an ID; report an incomplete scan if another device cannot provide one. Validate that a selectable output offers at least one `f32`, `i16` or `u16` configuration with 1–32 channels and a sample-rate range intersecting 8–384 kHz. This is setup compatibility, not proof of routing.
- Add one generated typed IPC query, `list_audio_devices`, returning `{ devices, incomplete }`; each device has `{ id, name, kind, input, output, configuration }`, with `configuration` equal to `supported`, `unsupported` or `unknown`. The query performs no stream creation, mic capture, persistence or OS-default changes. A whole-host enumeration failure maps to a new typed `deviceDiscoveryFailed` error (with EN/UK translations); partial device failures set `incomplete` or `unknown` without falsely saying no device exists.
- Test virtual/physical/unknown classification, role joining, ID failure, unsupported configurations and discovery while the existing output is playing. Run Rust fmt, Clippy and tests on the pinned toolchain. Draft PR 2 is based on the accepted-ADR branch until that PR merges.

## Change 2: frontend onboarding

- Add Settings > Audio Routing using existing `SettingsSection`, Base UI controls, Tailwind semantic tokens and i18next. On entry and on **Refresh Devices**, call `list_audio_devices`; use a monotonically increasing request token so a late earlier response cannot replace a newer one. Keep the selected ID only in React session state and mark it unavailable if absent after refresh.
- Derive the seven approved UX states from discovery result and session selection. Show local-playback availability separately. Device rows distinguish `Detected` from `Selected`; never render `Routing Active` or `Discord Verified` during this phase. Unknown-type outputs appear in an explicit manual-choice group; no physical output is auto-selected.
- Provide fixed official BlackHole installation and publisher-instruction links, manual steps, and **Refresh Devices**. The owner separately approved the Tauri opener dependency and capability change after this plan was written. Pin `@tauri-apps/plugin-opener` and `tauri-plugin-opener` at 2.7.2; grant only `opener:allow-open-url` scoped to `https://existential.audio/blackhole/` and `https://github.com/ExistentialAudio/BlackHole/wiki/Installation`. Do not grant `opener:default`, arbitrary URLs, paths or subprocess execution. Handle opener failures visibly and do not navigate the webview as a fallback.
- Add EN/UK translations and component tests for each state, refresh races, missing selection, unsupported configuration, keyboard focus and accessible status announcements. Check dark/light/system and the 640 × 480 minimum window on a real macOS device. Windows and Discord routing remain **NOT VERIFIED** without their own tests. Draft PR 3 is based on the backend branch until its PR merges.

## Acceptance and delivery

Local library playback works without a virtual device and through every discovery state. Discovery never starts capture or alters system defaults. Each PR reports exact checks and platform evidence, stays draft for review and is not merged automatically.

Sources: [CPAL 0.18.2 API](https://docs.rs/cpal/0.18.2/cpal/traits/trait.HostTrait.html), [Tauri opener permissions](https://v2.tauri.app/plugin/opener/).
