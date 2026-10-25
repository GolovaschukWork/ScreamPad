---
name: release
description: Prepare a release candidate; never publish without explicit approval.
---
# /release
1. Verify version, target channel, changelog, tag convention, and Git status.
2. Verify CI, real OS smoke tests, signing/notarization and updater signatures.
3. Require license and bundled FFmpeg compliance audit, provenance, SBOM, checksums, and rollback plan.
4. Prepare release notes/checklist and request owner approval before tag/publish/update manifest changes.
5. Never expose credentials or execute production release autonomously.
