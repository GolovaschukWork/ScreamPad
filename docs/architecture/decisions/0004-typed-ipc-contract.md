# ADR 0004: Typed IPC Contract

- Status: Proposed
- Date: 2026-10-25
- Owner: TBD

## Context
The React UI talks to the Rust backend through Tauri commands (request/response) and events (asynchronous state changes).
`CLAUDE.md` requires typed commands with generated TypeScript DTOs, and the frontend rules forbid duplicated backend DTOs.
The owner's stated condition for preferring `ts-rs`: it must give a **verifiable IPC contract without manual duplication of
command signatures**.

A technical spike compared the two candidates against Tauri 2.12.5 on macOS arm64 (Rust 1.99.1, Node 24.22.1, TypeScript 6.0.3).
Both were built in throw-away copies of the scaffold; neither is integrated into the application. The same contract was
implemented twice: a command with a struct request (`u32`, `Vec<String>`, `Option<String>`), a response with `u64` and `i64`
fields, a second command without arguments, a tagged-enum error type, one event with an enum payload.

## Options
- **Option A: `ts-rs` 12.0.1** (DTO types only).
- **Option B: `tauri-specta` 2.0.0-rc.26** with `specta` 2.0.0-rc.26 and `specta-typescript` 0.0.13 (types, typed commands and events).
- **Option C: `ts-rs` plus a project-owned macro** that registers commands and emits the TypeScript command map. Not built;
  noted because it is what Option A needs in order to meet the condition.

## Spike results

| Aspect | `ts-rs` | `tauri-specta` |
| --- | --- | --- |
| Maturity | Stable 12.x, about 20M downloads | Release candidate for Tauri 2 (`2.0.0-rc.26`, 2026-10-21). The crates.io "stable" `1.0.2` targets Tauri 1 and cannot be used |
| What is generated | One `.ts` file per DTO, via `cargo test` | One `bindings.ts` with DTOs, a typed `commands` object, typed `events`, and a small runtime helper |
| Command signatures | **Hand-written** in TypeScript (`invoke("ping", { req })` plus a `CommandMap`) | **Generated**; no duplication |
| Rust rename of a command argument (`req` to `request`) | **Not detected**: `tsc` still passes, the call fails at runtime | Bindings follow automatically (`{ request }`) |
| Adding a required DTO field | Detected by `tsc` at every caller after regeneration | Same, after regeneration |
| `u64` / `i64` fields | **Silently emits `bigint`**, but JSON delivers a `number`: wrong at runtime. Needs `#[ts(type = "number")]` on every such field | **Refuses to export** until the field is annotated (`#[specta(type = f64)]`). Safe default, same annotation burden |
| Error type (tagged enum) | Correct discriminated union | Correct, plus a `{ status: "ok" \| "error" }` wrapper for results |
| Events | Payload types only; event names and `listen` calls hand-written | Typed `events.playbackStateChanged.listen(...)` |
| Output noise | Clean | Splits request types into `_Serialize` / `_Deserialize` variants; `f64`/`f32` become `number \| null` (accurate for NaN, but noisy) |
| When bindings are generated | `cargo test` (CI-friendly, no app launch) | In the spike, at app start in debug builds. A test-based export was **not** verified |
| Added crates (unique, `cargo tree`) | +3 (282 to 285) | +14 (282 to 296) |
| Build impact | Negligible | Small (about 20 s cold on the spike machine, mostly shared Tauri compile) |

Not verified in the spike: an actual `invoke` round trip from the WebView, behavior on Windows, and generating bindings from
a test instead of launching the app.

## Decision
**Not yet decided; owner approval required.** Recommendation, with the reasoning:

1. The owner's condition is **not met by Option A on its own**: command names and argument keys stay duplicated by hand, and
   an argument rename is invisible to the TypeScript compiler. A generated or macro-registered command map (Option C) would be
   needed, and that is a small in-house reimplementation of what Option B provides.
2. Recommended: **Option B**, with these controls for the release-candidate risk:
   - pin the exact versions with `=` in `Cargo.toml` and keep `Cargo.lock` committed;
   - confine the specta derives to IPC adapter DTOs in `commands/` so `domain` and `application` stay independent of it
     (consistent with `.claude/rules/rust.md`); replacing it later touches only that layer;
   - commit the generated `bindings.ts` and fail CI when regeneration produces a diff;
   - give every `u64`/`i64` field an explicit number mapping and a range-checked conversion;
   - reassess when `tauri-specta` reaches a stable 2.x.
3. Acceptable fallback: **Option A plus a convention** (one `req` DTO per command) and a guard test that compares a Rust-side
   command-name list with the TypeScript `CommandMap`. It narrows but does not remove the duplication.

The IPC layer must not use both libraries.

## Consequences
- Option B: typed commands and events with no hand-written signatures; depends on a pre-1.0 library and 14 more crates; the
  bindings file shape may change between release candidates, so upgrades are deliberate PRs.
- Option A: stable and light, but contract drift between command signatures and the TypeScript side is caught only by tests
  we write and by runtime errors.
- Either way, 64-bit integers cross IPC as JSON numbers and must stay within 2^53 or be sent as strings.
- Rollback: the IPC layer is a thin adapter; switching libraries rewrites generated bindings and the TypeScript call sites.

## Validation
- Implementation PR must include: a round-trip test of one command and one event in the real WebView, negative tests for
  invalid input (typed `AppError`), a CI drift check on generated bindings, and a Windows run of the same checks.
- Windows 11 and Intel macOS behavior remains NOT VERIFIED until the CI matrix runs the implementation PR.
