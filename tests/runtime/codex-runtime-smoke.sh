#!/bin/sh

set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
fixture="$repo_root/tests/runtime/fixture"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/rosettai-codex-runtime.XXXXXX")"
trace="$test_root/codex-runtime.jsonl"
last_message="$test_root/codex-last-message.txt"

cleanup() {
  if [ "${KEEP_RUNTIME_FIXTURE:-0}" != "1" ]; then
    rm -rf -- "$test_root"
  else
    printf '%s\n' "Runtime fixture preserved at $test_root"
  fi
}
trap cleanup EXIT INT TERM

if ! command -v codex >/dev/null 2>&1; then
  printf '%s\n' "codex is required on PATH" >&2
  exit 2
fi

if ! command -v rai >/dev/null 2>&1; then
  printf '%s\n' "rai is required on PATH; run 'cargo install --path .' first" >&2
  exit 2
fi

cp -R "$fixture/." "$test_root"
git -C "$test_root" init -q
rai sync --repo "$test_root"

prompt='Verify the RosettAI runtime integration. Return the repository instruction marker. Explicitly use the $runtime-probe skill and return its marker. Call the runtime_probe MCP tool record_runtime_probe. Do not create or edit files yourself; the MCP tool is the only component expected to write a file.'

if ! codex exec \
  --cd "$test_root" \
  --json \
  --sandbox workspace-write \
  --dangerously-bypass-hook-trust \
  --output-last-message "$last_message" \
  "$prompt" >"$trace"; then
  printf '%s\n' "Codex runtime invocation failed. Trace: $trace" >&2
  KEEP_RUNTIME_FIXTURE=1
  exit 1
fi

assert_marker() {
  marker="$1"
  file="$2"
  if ! grep -Fq "$marker" "$file"; then
    printf '%s\n' "Missing runtime marker $marker in $file" >&2
    KEEP_RUNTIME_FIXTURE=1
    exit 1
  fi
}

assert_marker "ROSETTAI_AGENTS_LOADED" "$last_message"
assert_marker "ROSETTAI_SKILL_LOADED" "$last_message"
assert_marker "ROSETTAI_MCP_LOADED" "$test_root/mcp-runtime-proof.txt"
assert_marker 'record_runtime_probe' "$trace"

test -f "$test_root/.codex/agents/runtime_reviewer.toml"
assert_marker 'name = "runtime_reviewer"' "$test_root/.codex/agents/runtime_reviewer.toml"
assert_marker 'developer_instructions = "Return exactly ROSETTAI_SUBAGENT_LOADED when asked for the runtime subagent marker."' "$test_root/.codex/agents/runtime_reviewer.toml"

subagent_trace="$test_root/codex-subagent-context.jsonl"
subagent_message="$test_root/codex-subagent-context.txt"
subagent_prompt='Without opening project files or running commands, return the exact marker specified by the runtime_reviewer custom agent developer instructions.'
if ! codex exec \
  --cd "$test_root" \
  --json \
  --sandbox workspace-write \
  --dangerously-bypass-hook-trust \
  --output-last-message "$subagent_message" \
  "$subagent_prompt" >"$subagent_trace"; then
  printf '%s\n' "Codex subagent context invocation failed. Trace: $subagent_trace" >&2
  KEEP_RUNTIME_FIXTURE=1
  exit 1
fi
assert_marker "ROSETTAI_SUBAGENT_LOADED" "$subagent_message"
if grep -Fq '"type":"command_execution"' "$subagent_trace"; then
  printf '%s\n' "Subagent context probe read files through the shell instead of loaded Codex configuration." >&2
  KEEP_RUNTIME_FIXTURE=1
  exit 1
fi

printf '%s\n' "Codex runtime smoke test passed."
