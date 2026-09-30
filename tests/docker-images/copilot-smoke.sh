#!/bin/sh

set -eu

command -v copilot >/dev/null
command -v code >/dev/null
copilot --version
code --version
node -e "const p = require('/usr/share/code/resources/app/extensions/copilot/package.json'); if (p.publisher !== 'GitHub' || p.name !== 'copilot-chat') process.exit(1)"

test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT
mkdir -p "$test_root/.agents/rules" "$test_root/.agents/subagents"

cat > "$test_root/.agents/rules/testing.md" <<'EOF'
# Copilot container smoke test

Use the canonical RosettAI resources.
When asked for the Copilot smoke marker, reply exactly ROSETTAI_COPILOT_RULE_LOADED.
EOF

cat > "$test_root/.agents/subagents/reviewer.yaml" <<'EOF'
name: reviewer
description: Review code changes.
developer_instructions: Report concrete regressions.
EOF

rai sync --repo "$test_root"

grep -Fq 'Use the canonical RosettAI resources.' "$test_root/.github/copilot-instructions.md"
grep -Fq 'Report concrete regressions.' "$test_root/.github/agents/reviewer.agent.md"

printf '%s\n' "Copilot CLI, VS Code Copilot, and RosettAI projections passed."

if [ "${1:-}" = "--live" ]; then
  if [ -z "${COPILOT_GITHUB_TOKEN:-}${GH_TOKEN:-}${GITHUB_TOKEN:-}" ]; then
    printf '%s\n' 'Set COPILOT_GITHUB_TOKEN, GH_TOKEN, or GITHUB_TOKEN for the live test.' >&2
    exit 2
  fi
  result="$(cd "$test_root" && copilot -p 'Return the Copilot smoke marker from the project instructions. Reply with the marker only.')"
  printf '%s\n' "$result"
  printf '%s\n' "$result" | grep -Fq 'ROSETTAI_COPILOT_RULE_LOADED'
  printf '%s\n' 'Live Copilot CLI instruction-loading smoke test passed.'
fi
