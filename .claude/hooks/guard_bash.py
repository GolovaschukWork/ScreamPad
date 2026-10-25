#!/usr/bin/env python3
"""Defense-in-depth deny-list for Claude Code Bash calls; never a security sandbox."""
import json
import re
import sys

try:
    payload = json.load(sys.stdin)
except (json.JSONDecodeError, UnicodeDecodeError):
    print("Unable to parse tool call; refusing execution", file=sys.stderr)
    sys.exit(2)

cmd = str(payload.get("tool_input", {}).get("command", ""))
# Overly broad/ambiguous shell composition is not automatically trusted.
patterns = [
    r"\bgit\s+push\b[^\n]*?(?:--force|-f\b|--delete)",
    r"\bgit\s+push\b[^\n]*\b(?:main|master)\b",
    r"\bgh\s+pr\s+merge\b",
    r"\bgh\s+release\s+(?:create|delete)\b",
    r"\bgit\s+(?:reset\s+--hard|clean\s+-[a-z]*f)",
    r"\brm\s+-[a-z]*r[a-z]*f|\brm\s+-[a-z]*f[a-z]*r",
    r"\b(?:curl|wget)\b[^\n]*\|\s*(?:bash|sh)\b",
]
if any(re.search(p, cmd, re.IGNORECASE) for p in patterns):
    print("Blocked high-risk command; ask owner for an alternative workflow.", file=sys.stderr)
    sys.exit(2)

sys.exit(0)
