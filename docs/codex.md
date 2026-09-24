# Codex adapter

Tested with `codex-cli 0.152.1`; `rai sync` rejects older versions when Codex is enabled. Newer versions are accepted, but should be revalidated when Codex changes its configuration schema. Codex CLI, desktop, and IDE share the project configuration format, subject to each host's project-trust and hook settings.

Create `.agents/codex.json` containing `{}` to opt a repository into Codex. Then run `rai sync`. The existing Claude and Cursor projections remain in place for backward compatibility.

| Canonical resource | Codex behavior |
| --- | --- |
| `.agents/rules/*.md` | Combined into root `AGENTS.md` (global rules only). |
| `.agents/skills/<name>/SKILL.md` | Used in place by Codex; validated, not copied. |
| `.agents/mcp.json` | Converted to project `.codex/config.toml` MCP server entries. |
| `.agents/agents/<name>.json` | Converted to `.codex/agents/<name>.toml` custom subagents. |
| `.agents/codex.json` | Enables the projection and a `UserPromptSubmit` sync guard. |

The supported MCP schema is deliberately small. Use `{"servers":{"name":{"transport":"http","url":"https://example.com/mcp","bearer_token_env_var":"TOKEN"}}}` for HTTP, or `{"servers":{"name":{"transport":"stdio","command":"npx","args":["-y","package"],"env_vars":["TOKEN"]}}}` for STDIO. `cwd` is also supported for STDIO. Put secrets in environment variables, never canonical JSON. Unsupported fields are rejected rather than silently dropped. Agent JSON requires `name`, `description`, and `developer_instructions`; its name must match its filename.

The generated `UserPromptSubmit` hook runs `rai sync --codex-hook`. When anything changes, it blocks the prompt and asks for resubmission in a **new Codex session**. Codex loads `AGENTS.md`, skills, MCP and hooks at session start, so the hook cannot guarantee that a current session sees newly written files. Codex must trust the project and the hook before using them; a disabled or untrusted hook does not enforce synchronization. Git clone hooks and the watcher still provide early synchronization where installed. Run `rai status` to verify manually.

RosettAI refuses to overwrite tracked, user-owned, or modified projections. It only writes files with a verifiable ownership marker. A repository with an existing tracked `AGENTS.md` must migrate its instructions and remove that tracked native file before opting in. Codex `.codex/rules/*.rules` are execution-approval policies, not prose instructions; RosettAI does not translate Markdown rules into them.

The end-to-end fixture is [Gamma-Software/rosettai-clone-hook-e2e-20260924-214103](https://github.com/Gamma-Software/rosettai-clone-hook-e2e-20260924-214103). It contains one rule, skill, custom agent, HTTP MCP server and STDIO MCP server. A fresh clone with the RosettAI Git template hook projects them on checkout. The unit and CLI tests cover conversion, ownership conflicts, version parsing, idempotence and prompt-hook blocking.
