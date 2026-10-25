# Development workflow and approvals

1. Inspect Git status, branch, affected files, relevant instructions, and Accepted ADRs. Preserve unrelated work.
2. Assess scope and risk. State a short plan for normal work; obtain owner approval for a large architectural plan before implementation.
3. Obtain explicit owner approval before changing architecture or Accepted ADRs, schema or migrations, dependencies, security boundaries, permissions, signing, updater configuration, or release policy. Do not infer approval from tool access.
4. Implement small, focused changes. Add or update behavioral tests and documentation where appropriate.
5. Run relevant quality gates from the actual project scripts and CI. Review the diff and report failures and unverified areas truthfully.
6. When approved work is complete, create a Conventional Commit. For coherent work, push a short-lived feature branch and open a draft PR when GitHub access is available. Do not create a branch or PR for every microchange.

For small scoped tasks, proceed autonomously. Never merge, force-push, rewrite shared history, push directly to protected `main`, publish a release, or use destructive Git operations without explicit owner approval. Release preparation and publication follow `release-policy.md`.

Use pinned toolchains and actual project scripts. Do not invent a lint command when no linter is configured. Relevant checks include `pnpm typecheck`, `pnpm i18n:check`, `pnpm test:scripts`, `pnpm test:ui`, `pnpm build`, `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`. Report commands actually run; label others **NOT VERIFIED**.
