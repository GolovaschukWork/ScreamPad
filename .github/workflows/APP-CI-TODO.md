# Activate after application scaffold exists

Add `.github/workflows/ci.yml` with jobs for:
- pnpm install --frozen-lockfile, lint, typecheck, test, build
- cargo fmt --check, clippy -- -D warnings, test
- native Windows 11 x64 and macOS runners (arm64/x64, matching Tauri toolchains)
- caching, minimal permissions, pinned major/revision action versions

Do not label application CI as passing until actual scripts and builds exist.

Create a separate manually approved release workflow with signing secrets in protected environment. Never give signing credentials to PR triggers. CodeQL is provided for TS when files exist; evaluate Rust security tools separately.
