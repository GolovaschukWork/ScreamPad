# ADR register

Accepted decisions:
- 0001 Desktop stack and modular monolith
- 0002 Managed library and SQLite
- 0003 External virtual device routing
- 0004 Typed IPC contract with tauri-specta (owner chose option B)
- 0005 SQLite access with rusqlite and rusqlite_migration
- 0006 Audio engine threading, buffering, callback safety and device lifecycle (mixer implemented; device engine pending)
- 0007 Managed library import: atomicity, cleanup and recovery (accepted with conditions)
- 0008 Localization and translatable errors (English and Ukrainian, i18next)
- 0009 Backend-owned file selection (native dialog opened from Rust)
- 0010 Application settings and per-sound playback options (schema 2)
- 0011 UI stack (Tailwind v4, shadcn/ui on Base UI, Lucide, Sonner)
- 0012 Frontend component tests (Vitest) and in-app navigation without a router
- 0014 Virtual audio device onboarding (macOS, provider-neutral discovery and guided setup)

Proposed (awaiting owner decision):
- 0013 Microphone and virtual output routing (prototype and real-device measurements pending)
- 0015 Integrated call audio routing (macOS guided virtual-device setup; Windows existing-microphone feasibility)
- 0016 macOS integrated audio component (optional ScreamPad-branded GPL BlackHole build and consent-based Installer flow)

Future ADRs required for: URL providers, FFmpeg distribution, non-destructive edit schema, dependency compliance and signing provider. The owner selected GPL-3.0-only for ScreamPad on 2026-10-26; distribution compliance remains open.
