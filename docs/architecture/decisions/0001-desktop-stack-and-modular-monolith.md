# ADR 0001: Desktop Stack And Modular Monolith

- Status: Accepted (product discovery)
- Date: 2026-10-25

## Context
A cross-platform soundboard needs a maintainable, practical MVP.

## Decision
Use Tauri 2 + React + TypeScript + Rust, feature-based frontend and modular-monolith backend. Avoid premature crates. Alternatives: Electron, independent service processes. Consequence: native integrations must be tested per OS.

## Validation
Acceptance requires real-platform verification. This ADR records intent, not implementation success.
