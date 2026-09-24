# RosettAI: source configuration and local synchronization

## Purpose

Developers should be free to use different AI coding harnesses in the same repository. RosettAI is the translation layer between one shared configuration and the native formats required by each installed harness.

This document records the current design direction. A limited Rust CLI now implements `setup`, `init`, `status`, `sync`, and `doctor` for global Markdown rules. Setup can install a macOS polling watcher and Git hooks for future clones; scoped rules, native-file migration, and harness detection are not implemented. Where older architecture documents specify `.rosettai/` as the canonical directory or `rai run` as the primary activation path, this document supersedes those choices.

The proposed command interface is specified in [`cli.md`](cli.md).

## One source in `.agents/`

The team writes and versions its rules, Skills, hooks, plugins, and harness preferences under `.agents/`. RosettAI reads that directory as the source of truth. By default the POC projects global rules to `CLAUDE.md` and `.cursor/rules/rosettai.mdc`. An opt-in [Codex adapter](codex.md) also projects `AGENTS.md`, MCP configuration and subagents; Codex reads canonical skills directly. Detecting installed harnesses and producing `.opencode/` files remain future work.

After a normal Git clone, a locally installed RosettAI watcher can discover repositories containing `.agents/`, synchronize their projections, and check again when the source changes. The CLI is named `rai` (short for RosettAI): `rai setup` installs the local synchronizer once, while `rai sync` and `rai status` are available on demand. Future harness detection must not imply that every source feature can be translated; unsupported behavior must be reported.

## Activation components

- **CLI:** `rai setup` handles machine onboarding, `rai sync` performs an idempotent synchronization, `rai status` reports the effective state, and `rai doctor` diagnoses problems. The watcher and hooks are implementation details in the normal workflow.
- **Local discovery and change detection:** A machine-level RosettAI component finds repositories containing `.agents/` after clone and detects edits made outside Git. It invokes the same synchronization logic as the CLI.
- **Git hooks:** Where installed locally, `post-checkout` covers clone, branch switches, and worktree creation; `post-merge` covers successful merge-based pulls; `post-rewrite` covers rebases. Hooks call `rai sync` after the worktree changes. Git has no native `pre-fetch` or `pre-pull` hook, and fetch alone does not change the checked-out configuration.
- **Skills:** Optional integration for agents to explain or operate RosettAI. Skills must not be required for initial synchronization or for keeping native configuration current.

The Git hook integration must be installed on the developer's machine before cloning; hooks are not distributed by a repository. After a normal clone, `post-checkout` checks for `.agents/` and triggers synchronization so the detected harnesses receive their native files immediately. `git clone --no-checkout` does not trigger that hook. Hooks are an acceleration path, not the only trigger: the watcher also handles repositories cloned before installation, direct file edits, and Git workflows that do not run hooks. `rai setup` must preserve existing Git hooks and report when it cannot compose with their configuration.

## Generated files and Git

RosettAI checks the repository-root `.gitignore` before writing a projection. If an output path is not already ignored, it adds a rule for that **specific generated path** in a marked RosettAI block. It creates the root `.gitignore` if necessary and updates the block idempotently, preserving rules outside it. For example:

```gitignore
# RosettAI generated files
/CLAUDE.md
/.cursor/rules/rosettai.mdc
# End RosettAI generated files
```

Do not ignore an entire native directory such as `/.claude/` or `/.github/` merely because RosettAI writes one file there: the repository may contain hand-maintained files alongside generated ones. If a proposed output path already exists and is not owned by RosettAI, import it first when it is a supported native rule; otherwise report a conflict before writing. If Git already tracks that path, adding an ignore rule will not untrack it; report the tracked-output conflict.

RosettAI should record which paths it owns so later syncs can update or remove only its own outputs. `rai status` should show detected harnesses, generated paths, conflicts, and unsupported features.

## Native configuration drift

Every sync scans supported native instruction locations, including `CLAUDE.md` and `AGENTS.md` files in repository subdirectories. A file that RosettAI did not generate is an import candidate, even if it is ignored by Git. For example, `frontend/CLAUDE.md` may contain rules that belong in `.agents/rules/frontend.md`; its original directory scope must be preserved.

**Planned, not implemented:** `rai sync` should import supported files automatically; `rai sync --dry-run` should preview the canonical destination, scope, projection changes, and file replacement. An untracked original must be backed up locally before replacement. A Git-tracked original may be imported, but its native projection remains a reported conflict until it is removed from Git tracking. `rai doctor` should explain such conflicts and any content that the canonical model cannot express. RosettAI must not silently drop harness-specific behavior.
