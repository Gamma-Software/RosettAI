# `rai` CLI workflow

`rai` is the local command-line interface for RosettAI. The Rust proof of concept implements `setup`, `init`, `status`, `sync`, `doctor`, and the Cursor prompt guard with the options in the table below. A team versions its configuration in `.agents/`, while RosettAI projects global Markdown rules to Claude and Cursor files. Scoped rules, automatic migration of native files, and harness detection described below remain a product design.

Install the CLI with `cargo install --path .` before running `rai setup`. Setup registers workspace roots and a polling watcher; on macOS it installs a launchd agent. On other platforms, the watcher currently requires running `rai watch` manually. When Git has no global `core.hooksPath` or custom `init.templateDir`, setup installs hooks for future clones through a user-level template directory. It never replaces a configured global hook path or template directory.

## The normal path

1. **Set up the machine once:** run `rai setup`. It installs the per-user synchronizer, asks which workspace directories to watch, and arranges a local Git `post-checkout` hook for future clones when it can preserve existing hooks. `rai setup --root /path/to/workspace` supplies a directory non-interactively.
2. **Clone and work normally:** `git clone` puts `.agents/` on disk. Where setup installed a hook, Git invokes it after the initial checkout and RosettAI projects rules to Claude and Cursor files. The watcher also discovers clones under configured workspace directories.
3. **Change shared configuration:** edit `.agents/rules/*.md` and commit those source files. The watcher or hooks update generated files after local edits and Git operations. Native-rule import is not implemented yet.
4. **Check the result if needed:** run `rai status` from anywhere in the repository. It reports whether the two current projections need updating or have a conflict; `rai doctor` checks setup and source problems.

For a new repository, run `rai init` once to create a minimal `.agents/` source tree. It must not replace an existing `.agents/` directory. All repository commands find the nearest `.agents/` by walking upward; `--repo <path>` selects another checkout.

## Commands a developer may need

| Command | When to use it | Options |
| --- | --- | --- |
| `rai setup` | Once per machine, or when adding a workspace directory. | `--root <path>` (repeatable) |
| `rai init` | Start using RosettAI in a repository without `.agents/`. | `--repo <path>` |
| `rai status` | See the state of generated configuration and any drift. | `--repo <path>`, `--json` |
| `rai sync` | Project global Markdown rules now, or act as a Cursor prompt guard. | `--repo <path>`, `--dry-run`, `--json`, `--cursor-hook` |
| `rai doctor` | Explain each issue and offer interactive fixes where safe. | `--repo <path>`, `--json` |

In a terminal, `rai doctor` lists each issue with a proposed solution, then asks whether to fix a numbered issue, fix **all automatically fixable** issues, or do nothing. It can initialize missing `.agents/`, run `rai sync` for drift, or rerun `rai setup` for a missing watcher/configuration. It never moves or overwrites unmanaged native instructions automatically; those issues include manual steps. After a fix it checks again. `rai doctor --json` and non-interactive runs never prompt; JSON issues include `message`, `solution`, and `autoFixable`.

Every command also accepts `--perf`. It writes one `Time taken: 12.3 ms` line to stderr, including when the command fails, so `--json` on stdout stays parseable. `cargo run` compilation time is excluded because the timer starts inside `rai`. The internal long-running `rai watch --perf` reports the time taken by each scan (every 30 seconds).

**Planned, not implemented:** when sync finds an unmanaged native instruction file such as `frontend/CLAUDE.md`, it should import its content into `.agents/rules/`, preserving its directory scope, and update the native projection. Today sync reports a conflict for an unmanaged root `CLAUDE.md`; it does not yet scan nested native files. The future migration flow should back up untracked originals before replacement and report Git-tracked output conflicts.

`rai sync` runs the same source validation as `rai doctor` before writing. RosettAI adds only specific generated paths to a marked block in the repository-root `.gitignore`; this may leave a reviewable Git change until the block is committed. An unowned file that is not a supported migration source is reported as a conflict and is never overwritten.

## What runs automatically

The per-user watcher polls configured workspace directories and discovers new clones up to four directory levels deep. `rai setup` can install a Git hook integration on the developer's machine before a clone. The `post-checkout` hook runs after the initial checkout of a normal clone, branch switch, or worktree creation. It checks for `.agents/` and calls the same sync operation used by the CLI. `post-merge` and `post-rewrite` cover merge-based pulls and rebases. The hook reports sync failures without making a successfully cloned repository appear to have failed.

Git has no native `pre-fetch` or `pre-pull` hook. A fetch by itself does not change the checked-out files. `git clone --no-checkout` does not run `post-checkout`; the watcher can synchronize once `.agents/` appears in the worktree. Git hooks are machine-local and are not supplied by the cloned repository. `rai setup` must compose with any existing Git hook directory or template rather than replacing it; if that is not possible, it reports the limitation and relies on the watcher.

The watcher and hooks use one idempotent sync operation, so two triggers for the same change produce one effective update. Skills may help an agent explain RosettAI, but they are not required for synchronization. Developers should not need service or hook subcommands for ordinary use.

## Cursor prompt guard

Configure a project hook in `.cursor/hooks.json` after installing `rai` on the
machine:

```json
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [
      {
        "command": "rai sync --cursor-hook",
        "timeout": 30
      }
    ]
  }
}
```

The hook emits only Cursor's JSON response on stdout. If the projection is
already current, it returns `{"continue":true}`. If `.agents/rules/` changed,
it updates the owned projections, returns `continue: false`, and asks the user
to resubmit. Blocking the first submission is intentional: Cursor does not
guarantee that rules written inside `beforeSubmitPrompt` are re-resolved for
that same request. Conflicts and invalid canonical sources also block the prompt
without overwriting unmanaged files.

See [Cursor hook use cases](cursor-hook-use-cases.md) for complete executable
examples and the exact guarantee boundary.
