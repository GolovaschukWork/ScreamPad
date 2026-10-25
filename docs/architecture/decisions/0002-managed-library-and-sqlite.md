# ADR 0002: Managed Library And Sqlite

- Status: Accepted (product discovery)
- Date: 2026-10-25

## Context
A cross-platform soundboard needs a maintainable, practical MVP.

## Decision
Copy imported assets to app-owned storage and index with SQLite migrations. Alternatives: paths to original files, JSON metadata. Consequence: implement staging, duplicate detection, reconciliation and safe backups.

## Validation
Acceptance requires real-platform verification. This ADR records intent, not implementation success.
