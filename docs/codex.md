# Codex adapter

Tested with `codex-cli 0.152.1`; `rai sync` rejects older versions when Codex is enabled. Newer versions are accepted, but should be revalidated when Codex changes its configuration schema. Codex CLI, desktop, and IDE share the project configuration format, subject to each host's project-trust and hook settings.

Run `rai sync` in a repository containing Markdown rules under `.agents/rules/`. Codex is the only active target. No opt-in file is needed. Existing rai-owned Claude and Cursor projections from earlier versions are removed; user-owned or Git-tracked files are preserved.

| Canonical resource | Codex behavior |
| --- | --- |
| `.agents/rules/*.md` | Projected to root or directory-scoped `AGENTS.md` files. |
| `.agents/skills/<name>/SKILL.md` | Used in place by Codex; validated, not copied. |
| `.agents/mcp.yaml` | Converted to project `.codex/config.toml` MCP server entries. |
| `.agents/subagents/<name>.md` | Converted to `.codex/agents/<name>.toml` and registered in `.codex/config.toml`. |
| `rai sync --codex-hook` | Guards prompt submission when configured in the generated Codex project config. |

All rule Markdown files live directly in `.agents/rules/`. Without frontmatter, a rule contributes to root `AGENTS.md`. To target another directory, set `path` in the file:

```md
---
name: AGENTS.md
path: frontend/components
---
Follow the component conventions.
```

For example, this content in `.agents/rules/components.md` produces `frontend/components/AGENTS.md`. The path is relative to the repository root; `path: .` explicitly targets the root. The target directory must already exist. Multiple rules targeting the same file are combined in deterministic filename order. Nested source directories and invalid or escaping paths are rejected. RosettAI removes stale scoped projections only when they are still rai-owned and untracked.

`name` optionally records the original instruction filename. Accepted values are `AGENTS.md`, `CLAUDE.md`, and `copilot-instructions.md`; other explicit names fail validation before synchronization. Migration fills it with the original instruction filename, such as `AGENTS.md`, even when the canonical file is named `AGENTS-src.md`. Projections use it in the rule heading; older rules fall back to their filename. A rule declaring only `name` is global. Renaming a source file preserves its name and scope when the metadata stays the same. Unknown metadata fields, duplicate fields, and invalid names or paths are rejected before writing projections.

The Codex adapter writes `AGENTS.md` in the directory selected by `path`. For example, `.agents/rules/repository.md` with `name: AGENTS.md` and `path: .` contributes to root `AGENTS.md`. The canonical filename identifies the source file, and `name` supplies its section heading in that projection.

Codex applies an `AGENTS.md` according to its directory hierarchy and the session's working directory. Start a session in `frontend/` to load `frontend/AGENTS.md`; a session started at the repository root does not eagerly load every nested file.

Each subagent YAML object requires `name`, `description`, and `developer_instructions`; `name` must match the filename. Optional supported settings are `model`, `model_reasoning_effort`, `sandbox_mode`, and the string list `nickname_candidates`. RosettAI emits one standalone TOML configuration layer per subagent and explicitly registers it under `[agents.<name>]`. Codex loads these project-scoped files for spawned sessions; subagent content is never placed in `AGENTS.md`.

The supported MCP schema is deliberately small. Use `{"servers":{"name":{"transport":"http","url":"https://example.com/mcp","bearer_token_env_var":"TOKEN"}}}` for HTTP, or `{"servers":{"name":{"transport":"stdio","command":"npx","args":["-y","package"],"env_vars":["TOKEN"]}}}` for STDIO. `cwd` is also supported for STDIO. Either transport may set `default_tools_approval_mode` to `auto`, `prompt`, `writes`, or `approve`; use `approve` only for a server whose tools are trusted. Put secrets in environment variables, never canonical JSON. Unsupported fields are rejected rather than silently dropped.

The generated `UserPromptSubmit` hook runs `rai sync --codex-hook`. When anything changes, it blocks the prompt and asks for resubmission in a **new Codex session**. Codex loads `AGENTS.md`, skills, MCP and hooks at session start, so the hook cannot guarantee that a current session sees newly written files. Codex must trust the project and the hook before using them; a disabled or untrusted hook does not enforce synchronization. Git clone hooks and the watcher still provide early synchronization where installed. Run `rai status` to verify manually.

RosettAI refuses to overwrite tracked, user-owned, or modified projections. It only writes files with a verifiable ownership marker. A repository with an existing tracked `AGENTS.md` must migrate its instructions and remove that tracked native file before opting in. Codex `.codex/rules/*.rules` are execution-approval policies, not prose instructions; RosettAI does not translate Markdown rules into them.

The historical end-to-end fixture is [Gamma-Software/rosettai-clone-hook-e2e-20260924-214103](https://github.com/Gamma-Software/rosettai-clone-hook-e2e-20260924-214103). The local example in `tests/integration/fixtures/codex-sync/` has an `input/` project containing only supported `.agents/` resources and an `expected/` snapshot of the complete project after sync. The integration suite compares them byte-for-byte and also tests scoped rules, cleanup, validation, ownership, version compatibility, YAML subagent projection, and prompt-hook behavior. Run it with `cargo test --test codex_sync`. For a real Codex session, run `tests/runtime/codex-runtime-smoke.sh` as documented in `tests/integration/README.md`.
