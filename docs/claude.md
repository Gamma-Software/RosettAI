# Claude Code adapter

`rai sync` projects `.agents/` to Claude Code project configuration. Validated with Claude Code 2.1.258.

| Canonical resource | Claude Code projection |
| --- | --- |
| Global rules in `.agents/rules/*.md` | `CLAUDE.md` |
| Rules with `path: directory` | `.claude/rules/<directory>.md` with `paths: ["directory/**"]` |
| `.agents/subagents/<name>.md` | `.claude/agents/<name>.md` with `name`, `description`, and instructions |
| `.agents/skills/<name>/SKILL.md` | `.claude/skills/<name>/SKILL.md` |
| `.agents/mcp.yaml` | `.mcp.json` with `mcpServers` and `${ENV_VAR}` references |

The canonical agent's Codex-specific `model`, `model_reasoning_effort`, `sandbox_mode`, and `nickname_candidates` have no portable Claude mapping. Claude uses its selected model and permissions. Canonical MCP `cwd` and `default_tools_approval_mode` have no project-server equivalents; Claude runs the command in its own working directory and asks for project MCP approval. Skill attachments beyond `SKILL.md` are rejected during planning so references are not silently broken. Canonical hooks are not yet supported. The CLI generates `.claude/settings.json` with a `PreToolUse` guard for `Edit`, `Write`, and `MultiEdit`, invoking `rai guard`. It denies direct edits to active projections and redirects agents to canonical resources. Existing unowned project settings remain a conflict and are preserved; custom hooks are not merged automatically. The guard requires trusted, enabled hooks and does not intercept arbitrary shell/MCP writes. See [Claude hooks](https://code.claude.com/docs/en/hooks). No Claude prompt hook is installed; run `rai sync` before starting or restarting Claude to load current project configuration.

Generated files have integrity markers and exact-path `.gitignore` entries. An existing unowned, tracked, symlinked, or invalidly marked native file is a conflict. Active rai projections whose digest changed but whose marker remains intact are backed up externally, then regenerated from `.agents/`. Check loaded instructions and skills with `/context`, agents with `/agents`, and MCP with `claude mcp get <name>` or `/mcp`. A new session may be needed after synchronizing rules, skills, or MCP.

References: [project memory and scoped rules](https://code.claude.com/docs/en/memory), [agents](https://code.claude.com/docs/en/sub-agents), [skills](https://code.claude.com/docs/en/skills), and [MCP](https://code.claude.com/docs/en/mcp).
