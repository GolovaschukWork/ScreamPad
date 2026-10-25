# ADR 0003: External Virtual Audio Devices

- Status: Accepted (product discovery)
- Date: 2026-10-25

## Context
A cross-platform soundboard needs a maintainable, practical MVP.

## Decision
For MVP, target user-installed virtual audio devices rather than shipping a new driver. Support mic + clip mixing to device output. Alternatives: own kernel/system driver. Consequence: onboarding and platform-specific integration tests are required.

## Validation
Acceptance requires real-platform verification. This ADR records intent, not implementation success.
