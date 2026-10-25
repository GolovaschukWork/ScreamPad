#!/usr/bin/env bash
# Table test for detect-changes.sh. Usage: .github/scripts/test-detect-changes.sh
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
failed=0

check() {
  local name="$1" expected="$2" files="$3" actual
  actual="$(printf '%s\n' "$files" | "$here/detect-changes.sh" | tr '\n' ' ' | sed 's/ $//')"
  if [ "$actual" = "$expected" ]; then
    echo "ok   $name"
  else
    echo "FAIL $name: expected [$expected] got [$actual]" >&2
    failed=1
  fi
}

check "docs only" "frontend=false native=false codeql=false" $'docs/architecture/overview.md\nREADME.md\nCLAUDE.md'
check "adr only" "frontend=false native=false codeql=false" "docs/architecture/decisions/0005-sqlite.md"
check "governance only" "frontend=false native=false codeql=false" $'.claude/settings.json\n.claude/hooks/guard_bash.py\n.github/pull_request_template.md'
check "unrelated workflow" "frontend=false native=false codeql=false" ".github/workflows/governance.yml"
check "license" "frontend=false native=false codeql=false" "LICENSE"
check "gitignore" "frontend=false native=false codeql=false" ".gitignore"
check "ts source" "frontend=true native=false codeql=true" "src/app/App.tsx"
check "css only" "frontend=true native=false codeql=false" "src/app/App.css"
check "index.html" "frontend=true native=false codeql=false" "index.html"
check "rust source" "frontend=false native=true codeql=false" "src-tauri/src/lib.rs"
check "tauri conf" "frontend=false native=true codeql=false" "src-tauri/tauri.conf.json"
check "cargo lock" "frontend=false native=true codeql=false" "src-tauri/Cargo.lock"
check "rust toolchain" "frontend=false native=true codeql=false" "rust-toolchain.toml"
check "pnpm lock" "frontend=true native=true codeql=true" "pnpm-lock.yaml"
check "ipc bindings only" "frontend=true native=true codeql=true" "src/shared/ipc/bindings.ts"
check "ts and rust" "frontend=true native=true codeql=true" $'src/shared/ipc.ts\nsrc-tauri/src/lib.rs'
check "ci workflow" "frontend=true native=true codeql=false" ".github/workflows/ci.yml"
check "codeql workflow" "frontend=false native=false codeql=true" ".github/workflows/codeql.yml"
check "tsconfig" "frontend=true native=false codeql=false" "tsconfig.node.json"
check "empty diff" "frontend=false native=false codeql=false" ""
check "src lookalike dir" "frontend=false native=false codeql=false" "docs/src/notes.md"

exit "$failed"
