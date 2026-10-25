# Git and PR policy

- `main` is protected. Short-lived branches: `<type>/<kebab-case-subject>`.
- Types: feat, fix, perf, refactor, test, docs, ci, build, chore, revert.
- Conventional Commits with a relevant scope where meaningful; PR title must satisfy commitlint.
- Squash merges only; final squash message uses conventional format.
- Claude may branch, commit, push to its feature branch and open draft PRs, never merge or release autonomously.
- No force push, no direct main push; no unattended changes to protection/security settings.
- Required checks: lint, TS typecheck, frontend tests, Rust fmt/clippy/tests, Windows and macOS builds once source exists. Security checks additionally report via CodeQL/Dependabot.
- Prefer a bounded PR with acceptance criteria, testing evidence, screenshots for UI and platform caveats.
