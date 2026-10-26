# Open decisions — do not invent answers

1. Publisher identity, domain and the production app/bundle identifier: TBD. (Product name ScreamPad and the repository slug are decided; `soundboard-desktop` / `dev.local.soundboard` remain temporary internal identifiers.)
2. GPL-3.0-only dependency license audit and distribution obligations: exact FFmpeg configuration, bundled components, source and notices remain open.
3. URL imports: which providers and legal/technical boundaries are supported?
4. FFmpeg: executable vs linked libraries, codec subset, redistribution and compliance plan.
5. Audio stack compatibility spike: CPAL, device enumeration, virtual cable routing and mic echo prevention.
6. Typed IPC generator and migration tool compatibility with selected Tauri/Rust versions.
7. Windows signing vendor/cost/certificate storage; macOS Apple developer account and notarization secrets.
8. CI runner/OS smoke-test strategy (real Windows 11 and Intel/ARM Mac) and associated budget.
9. Precise latency/performance targets only after hardware benchmarking.
10. Product branding, color tokens, accessibility acceptance criteria and keyboard mapping.
