# RosettAI: source configuration and local synchronization

## Purpose

The current RosettAI implementation targets Codex, Claude Code, and GitHub Copilot Desktop.

This document records the current design direction. A limited Rust CLI now implements `install`, `uninstall`, `init`, `migrate`, `rollback`, `status`, `sync`, and `doctor` for global and directory-scoped Markdown rules. Install can set up a polling watcher on macOS, Linux, and Windows, plus Git hooks for future clones. Init detects native harness files and can import untracked root instructions after an explicit confirmation; structured configuration still requires manual migration. Where older architecture documents specify `.rosettai/` as the canonical directory or `rai run` as the primary activation path, this document supersedes those choices.

The proposed command interface is specified in [`cli.md`](cli.md).

## One source in `.agents/`

The team writes and versions its rules, subagents, Skills and MCP servers under `.agents/`. RosettAI reads that directory as the source of truth. The [Codex adapter](codex.md), [Claude Code adapter](claude.md), and [Copilot Desktop adapter](copilot-desktop.md) project them into each harness's native files. Other harness adapters remain future work.

### Canonical source formats

Use two formats for files that people write under `.agents/`:

| Resource | Canonical format | Reason |
| --- | --- | --- |
| Rules, subagents, commands, and Skills | Markdown (`.md`) with YAML frontmatter when metadata is needed | Keep instructions readable while giving adapters structured fields. |
| MCP servers, policies, and other data-only configuration | YAML (`.yaml`) | Express nested data without wrapping it in prose. |

For example, a canonical subagent can be written as `.agents/subagents/reviewer.md`:

```md
---
name: reviewer
description: Review changes before delivery
---

Check for defects and explain each finding with its location.
```

The YAML frontmatter describes the resource; the Markdown body contains its instructions. Omit frontmatter fields that are unnecessary for a resource. Keep secrets out of canonical files: use references such as `env:NAME` or `secret:provider/key` instead of literal values. Adapters validate these sources and generate the JSON, TOML, or other native files required by each harness. Generated formats do not determine the canonical source format.

**Implementation status:** the CLI accepts `.md` subagents and `.agents/mcp.yaml`. It also accepts legacy `.yaml` subagents and `.agents/mcp.json` while repositories migrate. Do not define the same subagent in both formats or include both MCP files. Policies and commands are still planned resources.

After a normal Git clone, a locally installed RosettAI watcher can discover repositories containing `.agents/`, synchronize their projections, and check again when the source changes. The CLI is named `rai` (short for RosettAI): `rai install` installs the local synchronizer once, while `rai sync` and `rai status` are available on demand. Future harness detection must not imply that every source feature can be translated; unsupported behavior must be reported.

## Activation components

- **CLI:** `rai install` handles machine onboarding, `rai sync` performs an idempotent synchronization, `rai status` reports the effective state, and `rai doctor` diagnoses problems. The watcher and hooks are implementation details in the normal workflow.
- **Local discovery and change detection:** A machine-level RosettAI component finds repositories containing `.agents/` after clone and detects edits made outside Git. It invokes the same synchronization logic as the CLI.
- **Git hooks:** Where installed locally, `post-checkout` covers clone, branch switches, and worktree creation; `post-merge` covers successful merge-based pulls; `post-rewrite` covers rebases. Hooks call `rai sync` after the worktree changes. Git has no native `pre-fetch` or `pre-pull` hook, and fetch alone does not change the checked-out configuration.
- **Skills:** Optional integration for agents to explain or operate RosettAI. Skills must not be required for initial synchronization or for keeping native configuration current.

The Git hook integration must be installed on the developer's machine before cloning; hooks are not distributed by a repository. After a normal clone, `post-checkout` checks for `.agents/` and triggers synchronization so the detected harnesses receive their native files immediately. `git clone --no-checkout` does not trigger that hook. Hooks are an acceleration path, not the only trigger: the watcher also handles repositories cloned before installation, direct file edits, and Git workflows that do not run hooks. `rai install` must preserve existing Git hooks and report when it cannot compose with their configuration.

## Generated files and Git

RosettAI checks the repository-root `.gitignore` before writing a projection. If an output path is not already ignored, it adds a rule for that **specific generated path** in a marked RosettAI block. It creates the root `.gitignore` if necessary and updates the block idempotently, preserving rules outside it. For example:

```gitignore
# RosettAI generated files
/AGENTS.md
/.codex/config.toml
# End RosettAI generated files
```

Do not ignore the entire `/.codex/` directory merely because RosettAI writes files there: the repository may contain hand-maintained files alongside generated ones. If a proposed output path already exists and is not owned by RosettAI, report a conflict before writing. If Git already tracks that path, adding an ignore rule will not untrack it; report the tracked-output conflict.

RosettAI should record which paths it owns so later syncs can update or remove only its own outputs. `rai status` should show detected harnesses, generated paths, conflicts, and unsupported features.

## Native configuration drift

Sync projects a flat Markdown source such as `.agents/rules/frontend.md` to `frontend/AGENTS.md` when it declares `path: frontend` in frontmatter, preserving directory scope. Manual sync also scans native `AGENTS.md` and `CLAUDE.md` files in repository subdirectories for migration. Canonical rules may declare `name` as one of the supported instruction filenames (`AGENTS.md`, `CLAUDE.md`, or `copilot-instructions.md`); migration preserves the original instruction filename in this field rather than the canonical storage filename; adapters use it as the rule label, while `path` determines scope. Both metadata fields remain optional for global rules, and older unnamed rules use their filename as the label. A file that RosettAI did not generate is an import candidate, even if it is ignored by Git.

**Planned, not implemented:** `rai sync` should import supported files automatically; `rai sync --dry-run` should preview the canonical destination, scope, projection changes, and file replacement. An untracked original must be backed up locally before replacement. A Git-tracked original may be imported, but its native projection remains a reported conflict until it is removed from Git tracking. `rai doctor` should explain such conflicts and any content that the canonical model cannot express. RosettAI must not silently drop harness-specific behavior.

### Typed resource directories

New source trees contain `rules/`, `agents/`, `commands/`, and `skills/`, each with an ignored `.keep` placeholder. Rules accept `type: rule` with optional `path`; the adapter chooses its native output filename. Files in `agents/` require `type: agent` alongside their existing agent metadata. Legacy `subagents/` and rules without a type remain readable. Duplicate agent names across the two agent directories are conflicts. Nonempty `commands/` currently blocks sync with an explicit unsupported-projection diagnostic; command adapters are not implemented.
