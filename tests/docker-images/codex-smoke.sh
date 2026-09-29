#!/bin/sh

set -eu

test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT

mkdir -p "$test_root/.agents/rules" "$test_root/.agents/skills/container-smoke"

cat > "$test_root/.agents/rules/testing.md" <<'EOF'
# Container smoke test

Use the canonical RosettAI resources.
EOF

cat > "$test_root/.agents/skills/container-smoke/SKILL.md" <<'EOF'
---
name: container-smoke
description: Reply with the exact container smoke-test marker when explicitly requested.
---

When asked to run this skill's smoke test, reply with exactly `ROSETTAI_SKILL_LOADED`.
EOF

rai sync --repo "$test_root"

test -f "$test_root/AGENTS.md"
test -f "$test_root/.codex/config.toml"
test -f "$test_root/.agents/skills/container-smoke/SKILL.md"
test ! -e "$test_root/.codex/skills/container-smoke"

if [ "${1:-}" != "--live" ]; then
  printf '%s\n' "Static Codex projection smoke test passed."
  printf '%s\n' "Run 'codex-smoke --live' with OPENAI_API_KEY to test real skill loading."
  exit 0
fi

if [ -z "${OPENAI_API_KEY:-}" ]; then
  printf '%s\n' "OPENAI_API_KEY is required for the live smoke test." >&2
  exit 2
fi

result="$(
  cd "$test_root"
  codex exec \
    --skip-git-repo-check \
    --sandbox read-only \
    "Use the container-smoke skill and run its smoke test. Return only its requested marker."
)"

printf '%s\n' "$result"
printf '%s\n' "$result" | grep -Fxq 'ROSETTAI_SKILL_LOADED'
printf '%s\n' "Live Codex skill-loading smoke test passed."
