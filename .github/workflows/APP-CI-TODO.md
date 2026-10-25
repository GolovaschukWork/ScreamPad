# Remaining CI work

`ci.yml` (frontend typecheck and build, native fmt/clippy/test on macOS arm64, macOS x64 and Windows x64) is active.

Still to add as the matching tools and approvals arrive:
- ESLint, Prettier and Vitest jobs, once those dependencies are approved.
- Caching for Cargo and pnpm if build times justify it.
- Dependabot `npm` and `cargo` ecosystems in `.github/dependabot.yml`.

Create a separate manually approved release workflow with signing secrets in a protected environment. Never give signing
credentials to PR triggers. The temporary identifier `dev.local.soundboard` must never be used for signing, the updater or releases.
