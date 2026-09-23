# RosettAI: source configuration and local synchronization

## Purpose

Developers should be free to use different AI coding harnesses in the same repository. RosettAI is the translation layer between one shared configuration and the native formats required by each installed harness.

This document records the current design direction. RosettAI is still in the architecture phase; the synchronization described here is not implemented. Where older architecture documents specify `.rosettai/` as the canonical directory or `rai run` as the primary activation path, this document supersedes those choices.

The proposed command interface is specified in [`cli.md`](cli.md).

## One source in `.agents/`

The team writes and versions its rules, Skills, hooks, plugins, and harness preferences under `.agents/`. RosettAI reads that directory as the source of truth. It detects supported harnesses installed on the developer's machine and produces the configuration each one expects, such as `AGENTS.md`, `CLAUDE.md`, or files under `.cursor/`, `.opencode/`, and `.codex/`.

After a normal Git clone, a locally installed RosettAI service discovers repositories containing `.agents/`, synchronizes their projections, and checks again when the source changes. The CLI is named `rai` (short for RosettAI); commands such as `rai sync` and `rai status` should expose synchronization and status on demand. Detection of an installed harness does not imply that every source feature can be translated; unsupported behavior must be reported.

## Activation components

- **CLI:** `rai sync` performs an idempotent synchronization; `rai status` reports the effective state and conflicts. The CLI is also the entry point for installation and troubleshooting.
- **Local discovery and change detection:** A machine-level RosettAI component finds repositories containing `.agents/` after clone and detects edits made outside Git. It invokes the same synchronization logic as the CLI.
- **Git hooks:** Where installed locally, `post-checkout` covers clone, branch switches, and worktree creation; `post-merge` covers successful merge-based pulls; `post-rewrite` covers rebases. Hooks call `rai sync` after the worktree changes. Git has no native `pre-fetch` or `pre-pull` hook, and fetch alone does not change the checked-out configuration.
- **Skills:** Optional integration for agents to explain or operate RosettAI. Skills must not be required for initial synchronization or for keeping native configuration current.

Hooks are an acceleration path, not the only trigger. RosettAI must still handle repositories cloned before installation, direct file edits, and Git workflows that do not run these hooks. Installing hooks must preserve any hooks already configured for the repository.

## Generated files and Git

RosettAI checks the repository-root `.gitignore` before writing a projection. If an output path is not already ignored, it adds a rule for that **specific generated path** in a marked RosettAI block. It creates the root `.gitignore` if necessary and updates the block idempotently, preserving rules outside it. For example:

```gitignore
# RosettAI generated files
/CLAUDE.md
/.cursor/rules/rosettai.mdc
# End RosettAI generated files
```

Do not ignore an entire native directory such as `/.claude/` or `/.github/` merely because RosettAI writes one file there: the repository may contain hand-maintained files alongside generated ones. If a proposed output path already exists and is not owned by RosettAI, report a conflict before writing. If Git already tracks that path, adding an ignore rule will not untrack it; report the conflict and require an explicit migration decision.

RosettAI should record which paths it owns so later syncs can update or remove only its own outputs. `rai status` should show detected harnesses, generated paths, conflicts, and unsupported features.

## Native configuration drift

RosettAI also scans supported native instruction locations, including `CLAUDE.md` and `AGENTS.md` files in repository subdirectories. A file that RosettAI did not generate is reported as unmanaged configuration, even if it is ignored by Git. For example, `frontend/CLAUDE.md` may contain rules that belong in `.agents/rules/frontend.md`; its original directory scope must be preserved during migration.

`rai doctor` should show the file, affected harnesses, effective scope, and a proposed canonical destination. `rai migrate` should preview the resulting source and projection changes, including removal of the original native file, before applying them. An untracked original must be backed up locally before removal. If the rule uses harness-specific behavior that the canonical model cannot express, RosettAI should retain an explicit harness-specific source or report the limitation.
