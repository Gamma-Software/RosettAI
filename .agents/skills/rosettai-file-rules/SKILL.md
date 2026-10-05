---
name: rosettai-file-rules
description: Create or edit canonical RosettAI resources in a repository's .agents/ directory, including rules, subagents, skills, and MCP servers. Use when an agent needs to define shared configuration for Codex, Claude Code, or Copilot through RosettAI.
---

# Define RosettAI resources

Write shared source files under the repository's `.agents/` directory. Treat generated `AGENTS.md`, `CLAUDE.md`, `.codex/`, `.claude/`, `.github/`, and `.vscode/` files as projections: inspect them when useful, but edit the canonical source. Read `docs/doc.md` in the RosettAI repository for the design and the relevant adapter page when a target's behavior matters. The older `docs/architecture/` proposal uses a different directory layout.

## Choose a source file

| Need | File | Content |
| --- | --- | --- |
| Repository or directory instructions | `.agents/rules/<name>.md` | Markdown; `type: rule` and optional `path` frontmatter. |
| Subagent | `.agents/agents/<name>.md` | YAML frontmatter for metadata; Markdown body for instructions. |
| Reusable skill | `.agents/skills/<name>/SKILL.md` | YAML frontmatter with `name` and `description`; Markdown instructions. |
| MCP servers | `.agents/mcp.yaml` | YAML data under the `servers` key. |

Use lowercase, descriptive names. The subagent and skill names must match their filename or directory name. Keep secrets out of source files; refer to environment variable names. Do not define a subagent in both `.md` and legacy `.yaml`, or keep both `.agents/mcp.yaml` and legacy `.agents/mcp.json`.

## Supported shapes

New rules declare `type: rule`; adapters choose native filenames. Existing rules without `type` remain readable. Legacy explicit `name` metadata accepts `AGENTS.md`, `CLAUDE.md`, or `copilot-instructions.md`; do not use it in new rules. A scoped rule declares `path: <directory>`; the directory must already exist. `path: .` means the repository root. The scope comes from `path`, regardless of the filename or `name`. Place rule files directly in `.agents/rules/`:

```md
---
type: rule
path: frontend
---

Use the project's frontend conventions.
```

An agent in `agents/` needs `type: agent`, `name` and `description` in frontmatter and a nonempty Markdown body. The current CLI also accepts `model`, `model_reasoning_effort`, `sandbox_mode`, and a nonempty list of `nickname_candidates`. Do not put `developer_instructions` or `tools` in the frontmatter; the body supplies the instructions, and `tools` is not a supported canonical field:

```md
---
type: agent
name: reviewer
description: Review changes before delivery
---

Check for regressions and cite the affected files.
```

A skill has its own `SKILL.md`. Its `name` and `description` frontmatter describe when an agent should use it; the body explains the task. Keep its instructions specific to the capability. The current Claude projection supports `SKILL.md` without attachments, so check target support before adding assets or references.

MCP configuration is data-only YAML. Supported transports are `http` with `url` and optional `bearer_token_env_var`, or `stdio` with `command` and optional `args`, `cwd`, and `env_vars`. Either may set `default_tools_approval_mode` to `auto`, `prompt`, `writes`, or `approve`. Use environment variable names rather than token values:

```yaml
servers:
  docs:
    transport: http
    url: https://example.com/mcp
    bearer_token_env_var: DOCS_TOKEN
```

Commands and policies are part of the planned format, but the CLI does not project them yet. Do not imply that adding such a file activates behavior.

## Check the result

Run `rai sync --dry-run` for the target repository, then inspect errors and the proposed outputs. Run `rai sync` when the task calls for applying the projection, and `rai status` to check drift. A sync may report a conflict for an existing native file that RosettAI does not own; preserve that file and report the conflict rather than replacing it. If the CLI is unavailable, validate the source shape against the current parser and say that projection was not verified.

New layouts also create an empty `commands/` directory. Command projections are not implemented; adding a command file blocks sync rather than silently ignoring it. Legacy `subagents/` remains readable, but duplicate names across it and `agents/` are conflicts. Rules accept `type: rule`; legacy rule metadata remains readable.
