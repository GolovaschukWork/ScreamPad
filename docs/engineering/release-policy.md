# Release policy

## Distribution
GitHub Releases, and a future first-party website linking to verified downloads. No Apple App Store or Microsoft Store.

## Targets
Windows 11 x64 installer; macOS 14+ arm64 and x64 DMG/app builds. Actual bundle formats must be validated on supported machines.

## Channels and versions
SemVer: `0.1.0-beta.1` prerelease, `0.1.0` stable. Two update channels; stable must not pick up beta. Keep update metadata channel-specific.

## Gates
1. Prepare changelog, version-sync check, clean tree and release candidate.
2. Required CI green and OS-specific installation/launch/routing smoke tests recorded.
3. Apple Developer ID signing + notarization; supported Windows code-signing provider; independently signed Tauri updater assets.
4. Verify artifact hashes, provenance, dependency audit, FFmpeg compliance, download URLs, updater manifest and rollback plan.
5. Owner explicitly approves tag creation/publication and updater publication.

Never expose credentials to PR workflows. Favor immutable artifacts; signing certificates and updater keys are separate. A successful CI build alone is insufficient to ship.
