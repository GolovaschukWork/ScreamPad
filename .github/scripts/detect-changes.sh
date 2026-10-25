#!/usr/bin/env bash
# Print `frontend=`, `native=` and `codeql=` (true/false) for a list of changed files read from stdin.
# Anything not matched by a pattern (docs, governance files, images) triggers no code job.
set -euo pipefail

files="$(cat)"

match() {
  if grep -Eq "$1" <<<"$files"; then echo true; else echo false; fi
}

# TypeScript app and its build inputs.
echo "frontend=$(match '^(src/|scripts/|index\.html$|package\.json$|pnpm-lock\.yaml$|tsconfig[^/]*\.json$|vite\.config\.ts$|\.nvmrc$|\.npmrc$|\.github/workflows/ci\.yml$|\.github/scripts/)')"
# Rust app and toolchain. Frontend-only edits do not need the three-OS native matrix.
echo "native=$(match '^(src-tauri/|src/shared/ipc/|rust-toolchain\.toml$|package\.json$|pnpm-lock\.yaml$|\.nvmrc$|\.github/workflows/ci\.yml$|\.github/scripts/)')"
# CodeQL analyzes TypeScript sources and their dependencies.
echo "codeql=$(match '^(src/.*\.(ts|tsx)$|package\.json$|pnpm-lock\.yaml$|\.github/workflows/codeql\.yml$|\.github/scripts/)')"
