# `rai` CLI workflow

`rai` is the local command-line interface for RosettAI. The Rust proof of concept implements `install`, `uninstall`, `init`, `status`, `sync`, `doctor`, and a Codex prompt guard. A team versions its configuration in `.agents/`, while RosettAI projects rules, subagents, skills, and MCP to Codex, Claude Code, and GitHub Copilot Desktop. Automatic migration of native files remains future work.

Install the CLI with `cargo install --path .` before running `rai install`. Setup registers workspace roots and a polling watcher: a launchd agent on macOS, a systemd user service on Linux, or a per-user scheduled task on Windows. Linux requires a running systemd user manager. When Git has no global `core.hooksPath` or custom `init.templateDir`, setup installs hooks for future clones through a user-level template directory. It never replaces a configured global hook path or template directory.

## The normal path

1. **Set up the machine once:** run `rai install`. It installs the per-user synchronizer, asks which workspace directories to watch, and arranges a local Git `post-checkout` hook for future clones when it can preserve existing hooks. `rai install --root /path/to/workspace` supplies a directory non-interactively.
2. **Clone and work normally:** `git clone` puts `.agents/` on disk. Where setup installed a hook, Git invokes it after the initial checkout and RosettAI projects Codex configuration. The watcher also discovers clones under configured workspace directories.
3. **Change shared configuration:** edit Markdown files directly in `.agents/rules/` and commit those source files. Add `path: frontend` in a rule's `---` frontmatter to project it to `frontend/AGENTS.md` when `frontend/` exists; omit the field for a global rule. The watcher or hooks update generated files after local edits and Git operations. Native-rule import is not implemented yet.
4. **Check the result if needed:** run `rai status` from anywhere in the repository. It reports whether the Codex projections need updating or have a conflict; `rai doctor` checks setup and source problems.

For a new repository, run `rai init` once to create a minimal `.agents/` source tree. It must not replace an existing `.agents/` directory. All repository commands find the nearest `.agents/` by walking upward; `--repo <path>` selects another checkout.

## Commands a developer may need

| Command | When to use it | Options |
| --- | --- | --- |
| `rai install` | Once per machine, or when adding a workspace directory. | `--root <path>` (repeatable) |
| `rai uninstall` | Stop the per-user watcher and remove the Git template. | None |
| `rai init` | Start using RosettAI in a repository without `.agents/`. | `--repo <path>` |
| `rai status` | See the state of generated configuration and any drift. | `--repo <path>`, `--json` |
| `rai sync` | Project Codex configuration now, or act as a Codex prompt guard. | `--repo <path>`, `--dry-run`, `--json`, `--codex-hook` |
| `rai doctor` | Explain each issue and offer interactive fixes where safe. | `--repo <path>`, `--json` |
| `rai update` | Download, verify, and install the latest stable CLI release. | None |

Run `rai uninstall` to stop and remove the per-user watcher and the RosettAI Git template. It removes the global `init.templateDir` setting only when it points to that template. It leaves workspace roots, repository hooks and generated harness configuration, `.agents/` sources, and the `rai` executable untouched.

`rai install` records each installed Git template hook (`post-checkout`, `post-merge`, and `post-rewrite`) with its event and path in the per-user `installed-hooks.json` file (`~/.rai/` by default on macOS and Linux, `%APPDATA%\rai\` on Windows). `rai uninstall` removes records for hooks it removes. The registry covers hooks installed by the machine-level installer; generated project hooks remain part of their project configuration. On Unix, an explicit `XDG_CONFIG_HOME` uses `$XDG_CONFIG_HOME/rai/` instead. Existing `~/.config/rai/` data migrates to `~/.rai/` when no XDG override is set; conflicting files are preserved and reported. The release-check cache defaults to `~/.rai/cache/` on Unix, or `$XDG_CACHE_HOME/rai/` when set.

`rai update` downloads the release archive for the current platform and `SHA256SUMS`, verifies the archive's SHA-256, then replaces the running CLI executable. On Windows, a background PowerShell process finishes replacement after `rai` exits. The command needs write permission in the executable's directory and supports only the platforms published by the release workflow. It does not update Cargo's installation metadata; a later `cargo install --path .` can replace the downloaded binary.

Every normal `rai` invocation also checks for a newer stable release and prints a yellow warning at the end when one exists. The automatic check uses a one-hour cache and a two-second network timeout; connection failures do not affect the command. Warnings go to stderr, leaving `--json` output on stdout intact. The long-running internal `rai watch` process checks when it exits; `rai update` always requests a fresh result.

In a terminal, `rai doctor` asks separately about each missing workspace: provide its new path, stop watching it, or keep the entry for later. It then lists remaining issues and asks whether to fix a numbered issue, fix **all automatically fixable** issues, or do nothing. It can initialize missing `.agents/`, run `rai sync` for drift, or rerun `rai install` for a missing watcher/configuration. It never moves or overwrites unmanaged native instructions automatically; those issues include manual steps. After a fix it checks again. `rai doctor --json` and non-interactive runs never prompt; JSON issues include `message`, `solution`, and `autoFixable`. Missing workspaces have `autoFixable: false` because they require a choice.

Every command also accepts `--perf`. It writes one `Time taken: 12.3 ms` line to stderr, including when the command fails, so `--json` on stdout stays parseable. `cargo run` compilation time is excluded because the timer starts inside `rai`. The internal long-running `rai watch --perf` reports the time taken by each scan (every 30 seconds).

**Planned, not implemented:** when sync finds an unmanaged native instruction file such as `frontend/AGENTS.md`, it should import its content into `.agents/rules/`, preserving its directory scope, and update the native projection. Today sync reports a conflict for an unmanaged root `AGENTS.md`; it does not yet scan nested native files. The future migration flow should back up untracked originals before replacement and report Git-tracked output conflicts.

`rai sync` runs the same source validation as `rai doctor` before writing. RosettAI adds only specific generated paths to a marked block in the repository-root `.gitignore`; this may leave a reviewable Git change until the block is committed. An unowned file that is not a supported migration source is reported as a conflict and is never overwritten.

## What runs automatically

The per-user watcher polls configured workspace directories and discovers new clones up to four directory levels deep. `rai install` can install a Git hook integration on the developer's machine before a clone. The `post-checkout` hook runs after the initial checkout of a normal clone, branch switch, or worktree creation. It checks for `.agents/` and calls the same sync operation used by the CLI. `post-merge` and `post-rewrite` cover merge-based pulls and rebases. The hook reports sync failures without making a successfully cloned repository appear to have failed.

`rai install` records each workspace's filesystem identity in `root-identities.json` next to `roots.txt`. If a configured path disappears, the watcher searches near its old location and under the user's home directory every five minutes; `rai doctor` searches immediately. A unique match updates `roots.txt`. If no match is found, `rai doctor` reports the missing workspace and keeps its entry so it can be restored or corrected. The search is bounded to 20,000 directories and nine levels per search root; moves outside those areas, moves across filesystems, inaccessible folders, and copies may need a new `rai install --root PATH` and removal of the stale entry. On Unix, identity uses device and inode; on Windows, it uses the volume and directory creation timestamp, and ambiguous matches are rejected. Existing installations gain identity records when their configured workspaces are next found at their saved paths.

Git has no native `pre-fetch` or `pre-pull` hook. A fetch by itself does not change the checked-out files. `git clone --no-checkout` does not run `post-checkout`; the watcher can synchronize once `.agents/` appears in the worktree. Git hooks are machine-local and are not supplied by the cloned repository. `rai install` must compose with any existing Git hook directory or template rather than replacing it; if that is not possible, it reports the limitation and relies on the watcher.

The watcher and hooks use one idempotent sync operation, so two triggers for the same change produce one effective update. Skills may help an agent explain RosettAI, but they are not required for synchronization. Developers should not need service or hook subcommands for ordinary use.

## Codex prompt guard

The generated `.codex/config.toml` declares a `UserPromptSubmit` command hook
that runs `rai sync --codex-hook`. When synchronization changes files, the hook
blocks the prompt and asks for a new Codex session. Project configuration and
the hook must be trusted by Codex before this guard runs. See [Codex workflow](codex.md).
