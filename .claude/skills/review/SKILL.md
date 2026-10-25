---
name: review
description: Review code changes for correctness, maintainability and risk.
---
# /review
1. Read `git diff` and relevant code/contracts; do not edit unless explicitly asked.
2. Prioritize bugs, security, race conditions, broken contracts, tests, platform assumptions, and UX regressions.
3. Output findings by severity with file/line, impact, and concrete fix.
4. Explicitly say when no findings were identified, including residual risks.
