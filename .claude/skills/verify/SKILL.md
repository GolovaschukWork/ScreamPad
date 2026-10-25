---
name: verify
description: Run the relevant repository checks and report evidence.
---
# /verify
1. Determine changed files and available scripts; avoid inventing commands.
2. Run affected format/lint/typecheck/unit tests and builds when feasible.
3. Identify OS/hardware/signing validations not run; do not imply success.
4. Report PASS / FAIL / NOT RUN with exact commands and failure details.
