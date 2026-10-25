# Git guardrails
- Always inspect `git status` before branch or commit operations; preserve unrelated user changes.
- New branches: `feat/`, `fix/`, `perf/`, `refactor/`, `test/`, `docs/`, `ci/`, `build/`, `chore/`, `revert/` with kebab-case descriptions.
- Conventional Commits with meaningful scopes; keep one coherent task per PR.
- Branch/commit/push to feature branches and draft PR creation are permitted, subject to actual Git/CLI permissions.
- Never merge PRs, push main, force push, rewrite shared history, delete remote refs, or publish release without explicit approval.
- PR must describe scope, tests, risks, and platform verification; squash title must pass commitlint.
