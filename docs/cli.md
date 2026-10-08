# `rai` CLI workflow

Run `rai` in a terminal for an English-language interactive helper. It offers project configuration, synchronization, diagnosis, and machine management. Run `rai help` for the command list. When stdin is non-interactive, `rai` prints the same help instead of waiting for input. Direct commands remain available for scripts and experienced users. A mistyped command such as `rai syn` suggests the nearest command when the match is unambiguous. In a terminal, it offers to run that command with the original options after an explicit `y` or `yes`; Enter or EOF cancels. Non-interactive, JSON, and hook invocations only display the suggestion.

`rai` is the local command-line interface for RosettAI. The Rust proof of concept implements `install`, `uninstall`, `init`, `migrate`, `rollback`, `status`, `sync`, `doctor`, and a Codex prompt guard. A team versions its configuration in `.agents/`, while RosettAI projects rules, subagents, skills, and MCP to Codex, Claude Code, and GitHub Copilot Desktop.

Install the CLI with `cargo install --path .`, then run `rai install` once to enable global Git hooks for existing repositories and future clones. Pass `--root PATH` to also register a workspace root and polling watcher for projects without Git. Install checks existing repositories under selected roots for native harness configuration that needs migration before registering roots or installing the watcher. The watcher runs as a launchd agent on macOS, a systemd user service on Linux, or a per-user scheduled task on Windows. Linux requires a running systemd user manager. RosettAI sets a global `core.hooksPath` and delegates to any previous global hook directory. It also delegates to repository hooks when no global hook directory was previously configured. A repository-level `core.hooksPath` override takes precedence over the global setting.

## The normal path

1. **Set up and synchronize a project:** run `rai sync`. If native harness configuration exists, it displays the migration plan and asks before changing files. On first use in a Git repository it enables global Git hooks; in a non-Git project it registers that project with the watcher. Install the CLI first with `cargo install --path .`. Run `rai install --root /path/to/workspace` to add a directory to the watcher when needed.
2. **Clone and work normally:** `git clone` puts `.agents/` on disk. With global hooks installed, Git invokes RosettAI after the initial checkout and it projects harness configuration from `.agents/`. The watcher handles configured non-Git directories.
3. **Change shared configuration:** edit Markdown files directly in `.agents/rules/` and commit those source files. Add `path: frontend` in a rule's `---` frontmatter to project it to `frontend/AGENTS.md` when `frontend/` exists; omit the field for a global rule. The watcher or hooks update generated files after local edits and Git operations. Scoped native-rule import is not implemented yet.
4. **Check the result if needed:** run `rai status` from anywhere in the repository. It reports whether the Codex projections need updating or have a conflict; `rai doctor` checks setup and source problems.

For a new repository, the first manual `rai sync` creates `.agents/rules/`, `.agents/agents/`, `.agents/commands/`, and `.agents/skills/`, each with a `.keep` file, plus `.agents/mcp.yaml` containing `servers: {}`. It creates no default rule. The `.keep` files and empty MCP manifest produce no rules, agents, skills, or MCP projections. Sync automatically removes regular `.keep` files from subdirectories of `.agents/` that contain another entry, including nested skill directories and legacy `subagents/`. Empty directories retain their placeholders, and symbolic links are neither followed nor removed. These deletions appear in the sync report and JSON; `--dry-run` and `status` only preview them, and a blocked sync leaves them unchanged. `rai init` remains available for setup without immediate synchronization and must not replace an existing `.agents/` directory. All repository commands find the nearest `.agents/` by walking upward; `--repo <path>` selects another checkout.

## Commands a developer may need

| Command | When to use it | Options |
| --- | --- | --- |
| `rai install` | Enable global Git hooks; optionally add a watched directory. | `--root <path>` (repeatable) |
| `rai uninstall` | Remove global hooks and the optional watcher. | None |
| `rai migrate` | Run the migration step directly; `rai init` invokes it when needed. | `--repo <path>` |
| `rai rollback` | Restore native instructions from the recorded migration backup. | `--repo <path>` |
| `rai init` | Create an empty source tree without synchronizing. | `--repo <path>` |
| `rai status` | See the state of generated configuration and any drift. | `--repo <path>`, `--json` |
| `rai sync` | Set up an unconfigured project on manual use, then project configuration. Git hooks use a non-interactive mode. | `--repo <path>`, `--dry-run`, `--compact`, `--json`, `--codex-hook` |
| `rai doctor` | Explain each issue and offer interactive fixes where safe. | `--repo <path>`, `--json` |
| `rai update` | Download, verify, and install the latest stable CLI release. | None |
| `rai version` | Show the CLI version and the commit used to build the executable. | `--perf` |

`rai version` (also `rai --version` or `rai -V`) prints the package version and
the full Git commit SHA embedded at compilation. It works outside a project and
does not query Git or check for updates at runtime. `(dirty)` means the source
checkout contained uncommitted changes when compiled. Source archives without
Git metadata report `commit: unknown`; set `RAI_BUILD_SHA` to the full commit SHA
at build time to supply provenance for those builds. Already installed binaries
need to be rebuilt or updated before they support this command.

Run `rai uninstall` to stop the optional watcher and remove RosettAI global hooks. It restores the previous global `core.hooksPath` setting when RosettAI still owns it, and removes any legacy Git template it owns. It leaves workspace roots, repository hooks and generated harness configuration, `.agents/` sources, and the `rai` executable untouched.

`rai install` stores the previous global hook path in `global-hooks-previous.json`. Its global hook directory forwards other standard Git hooks to the previously configured global directory, or to repository hooks if no global directory was configured. Legacy template hooks (`post-checkout`, `post-merge`, and `post-rewrite`) remain recorded in the per-user `installed-hooks.json` file (`~/.rai/` by default on macOS and Linux, `%APPDATA%\rai\` on Windows). `rai uninstall` removes records for hooks it removes. The registry covers hooks installed by the machine-level installer; generated project hooks remain part of their project configuration. On Unix, an explicit `XDG_CONFIG_HOME` uses `$XDG_CONFIG_HOME/rai/` instead. Existing `~/.config/rai/` data migrates to `~/.rai/` when no XDG override is set; conflicting files are preserved and reported. The release-check cache defaults to `~/.rai/cache/` on Unix, or `$XDG_CACHE_HOME/rai/` when set.

`rai update` downloads the release archive for the current platform and `SHA256SUMS`, verifies the archive's SHA-256, then replaces the running CLI executable. On Windows, a background PowerShell process finishes replacement after `rai` exits. The command needs write permission in the executable's directory and supports only the platforms published by the release workflow. It does not update Cargo's installation metadata; a later `cargo install --path .` can replace the downloaded binary.

Every normal `rai` invocation also checks for a newer stable release and prints a yellow warning at the end when one exists. The automatic check uses a one-hour cache and a two-second network timeout; connection failures do not affect the command. Warnings go to stderr, leaving `--json` output on stdout intact. The long-running internal `rai watch` process checks when it exits; `rai update` always requests a fresh result.

In a terminal, `rai doctor` asks separately about each missing workspace: provide its new path, stop watching it, or keep the entry for later. It then lists remaining issues and asks whether to fix a numbered issue, fix **all automatically fixable** issues, or do nothing. It can initialize missing `.agents/`, run `rai sync` for drift, or rerun `rai install` for a missing watcher/configuration. It never moves or overwrites unmanaged native instructions automatically; those issues include manual steps. After a fix it checks again. `rai doctor --json` and non-interactive runs never prompt; JSON issues include `message`, `solution`, and `autoFixable`. Missing workspaces have `autoFixable: false` because they require a choice.

Every command also accepts `--perf`. It writes one `Time taken: 12.3 ms` line to stderr, including when the command fails, so `--json` on stdout stays parseable. `cargo run` compilation time is excluded because the timer starts inside `rai`. The internal long-running `rai watch --perf` reports the time taken by each scan (every 30 seconds).

Every manual `rai sync` checks for unmanaged root and nested instructions and native configuration under `.claude/`, `.codex/`, `.github/`, and `.cursor/`, including when `.agents/` already exists. Git hook synchronization never creates `.agents/` or asks migration questions. When migration is possible, it first lists every unmigrated native configuration file discovered. In a Git repository, it also reports a missing or outdated `.gitignore`. The migration plan lists the source and destination of each instruction import, tracked-file removal, and the `.gitignore` entries to add, then waits for an explicit `y` or `yes`; Enter, `n`, or EOF leaves the repository unchanged. The same review applies to `rai migrate`. After an approved migration, manual `rai sync` removes verified tracked originals from the Git index and working tree, then asks whether to synchronize now. The approved migration creates or updates the managed `.gitignore` block in Git repositories; `rai rollback` restores its previous contents or removes a file created by the migration, provided it has not changed since. Non-Git projects do not create or update `.gitignore`. The resulting projections are ignored by Git. It imports root `AGENTS.md`, `CLAUDE.md`, and `.github/copilot-instructions.md` into a global canonical rule with the original instruction filename in `name` metadata. An explicit `name` must be `AGENTS.md`, `CLAUDE.md`, or `copilot-instructions.md`; it records the original instruction filename and labels the projected rule. `path` controls directory scope. Manual sync validates existing canonical rules before offering migration, and an unsupported name blocks synchronization with the source path and accepted values. `rai doctor` reports the same error with repair guidance. Existing rules without `name` continue to use their filename as the label. Nested `AGENTS.md` and `CLAUDE.md` become separate canonical rules named `AGENTS-<relative-directory-with-slashes-replaced-by-hyphens>.md` (for example, `src/a/b/AGENTS.md` becomes `.agents/rules/AGENTS-src-a-b.md`), with the original instruction filename in `name` and `path: <directory>` frontmatter (for example, `.agents/rules/AGENTS-src.md` declares `name: AGENTS.md` and `path: src`); instructions from the same directory are combined without leaking into the repository-wide rules. When several instruction files share a directory, the rule name prefers `AGENTS.md`, then `CLAUDE.md`, then `copilot-instructions.md`. If two directories produce the same filename, or that canonical filename already exists, migration reports a conflict before writing any files. Additional native instructions can be migrated after an earlier migration; existing canonical rules and owned projections are preserved. Originals and `manifest.json` are saved outside the repository under `~/.rai/migrations/<project-name>/YYYY-MM-DD_HH-MM-SS/` on Unix (under `%APPDATA%\rai\migrations\<project-name>\` on Windows). If that second is already used, the name receives a numeric suffix. The manifest records the canonical repository path so `rai rollback` finds the matching backup even when two projects have the same name. It can also read backups created earlier directly under `migrations/` with a timestamp, short SHA-1, or full SHA-256 directory name, or in `.agents/migration-backup/`. If a recorded migration has lost its generated rule, interactive `rai sync` verifies the backup and sources, then offers to restore the rule and finish migration. It refuses changed sources or a changed generated rule. `rai rollback` verifies the backup and generated rule before restoring originals; it refuses to overwrite user changes. Structured settings, harness-specific rule directories, agents, or skills require manual conversion; migration reports them without changing the repository. Recursive discovery skips `.git/`, `.agents/`, harness internals, `node_modules/`, `target/`, and `fixtures/`; it stops at nested Git repositories or projects with their own `.agents/`, and never follows directory symlinks. Nested source symlinks are reported as conflicts. Version 2 manifests record every generated rule and its scope; rollback restores every original in the latest migration while preserving earlier canonical rules.

`rai sync` validates canonical sources and destinations before writing; it does not run the interactive `rai doctor` workflow. RosettAI adds only specific generated paths to a marked block in the repository-root `.gitignore` when the project is a Git repository; this may leave a reviewable Git change until the block is committed. An unowned file that is not a supported migration source is reported as a conflict and is never overwritten.

The terminal report starts with the synchronization verdict and counters for
created, updated, removed, and already synchronized files. It groups outputs by
agent: Codex, Claude Code, and GitHub Copilot. Project maintenance has its own
group for `.gitignore`, source placeholders, and other maintenance actions.
Each entry shows its full repository-relative path and explains its state;
the report includes planned or removed outputs and does not list unrelated
project files. Normal entries omit
ownership labels such as "managed by rai"; warnings retain ownership diagnostics.
Skills appear as one directory entry per skill, without listing their contents
or content comparisons. A conflicting skill file remains visible with its warning,
comparison and repair guidance. Green checks identify rai-managed
files whose ownership marker is valid and whose contents already match the expected
projection. Cyan entries identify created or updated files, and yellow entries
identify obsolete managed files removed during synchronization. A yellow warning
marks each conflicting file (local edits, missing ownership markers, Git tracking,
or symbolic links). A conflict no longer stops inspection of the other outputs:
the same report shows passing files and pending creations, updates, and removals,
with a summary of synchronized files and warnings. Unowned, tracked, symlinked, unreadable, or invalidly marked outputs still block
writes; pending actions are displayed as a preview. An active generated projection
with a syntactically intact rai marker but a changed digest is now automatically
resynchronized: rai saves its exact local contents outside the repository before
regenerating it from `.agents/`. All required backups must succeed before any
project output is replaced. The report shows the comparison, the restored file,
and its backup path. Local edits are not imported into canonical resources.
A missing/malformed marker and a modified obsolete output still require review.
Edited projections are not offered for automatic migration.
Each warning includes a way to resolve it. For edited or unowned files, the report
offers two choices: copy intended edits to the matching canonical resources
(preserving directory scope), then move the native file to a backup outside the
repository; or use the canonical version after moving the native file aside.
Tracked outputs include a `git rm --cached` command that keeps the local file
and stages its removal from Git. Symbolic-link, file-access, and malformed-ignore
warnings have specific repair guidance. The report gives commands to preview and
rerun sync after the warnings are resolved. Suggestions are never executed by
the report. Failed sync commands also point to `rai doctor` for guided diagnosis;
validation failures include a suggested repair. JSON output stays structured and
does not include the terminal guidance. An update requiring preservation has a
`backup` path in its change object; with `--dry-run` this is a planned path and no
backup or output is written. On successful sync the backup exists. Backups live
under `~/.rai/projection-backups/` (or the configured XDG directory on Unix,
`%APPDATA%\rai\projection-backups\` on Windows), grouped by repository and local
content digest, preserving the native relative path. They are kept for manual
review; repeated clean syncs create no further backup.
Only when a local file differs from its expected projection and both are readable, the report
also compares their contents and shows a similarity percentage plus the number
of local lines added and removed relative to the expected output. Similarity is
`2 × matching lines / (local lines + expected lines)`, with matching lines counted
in order. Ownership digest headers are excluded; valid JSON has its ownership
metadata removed and is formatted consistently before comparison. Invalid JSON
falls back to raw text comparison. Line endings and
the final newline do not affect this line-based comparison. A successful update
labels the comparison as "before sync". A 100% content match does not override an
ownership or Git-tracking conflict. Missing files, symbolic links and unreadable
files and obsolete outputs have no percentage; unusually large differences report that comparison is
unavailable instead of inventing a result.
For `.gitignore`,
the report refers specifically to the rai-managed ignore entries. The verdict
states whether any files changed. `--dry-run` describes proposed actions without
writing; `--json` retains its structured action names and includes no styling.

Use `rai sync --compact` for a compact receipt: it hides unchanged file entries
while retaining their count, all changed and pending paths, diagnostics,
comparisons, repair guidance, and backup paths. A blocked sync still explains
its conflicts and reports that no files changed. When everything is already
synchronized, the receipt shows the verdict and unchanged count without listing
each file. `--compact` affects only presentation, not synchronization behavior.
It is valid only with `sync` and can be combined with `--dry-run`, `--json`,
and hook modes. With `--json`, `--compact` is ignored and the JSON output remains
identical; the Codex hook's structured output also remains unchanged.

```sh
rai sync --compact
rai sync --dry-run --compact
rai sync --json --compact
```

`rai doctor` groups terminal diagnostics into Project, Synchronization, and Installation.
It displays green checks for passing groups, red errors for invalid resources or
unsafe projections, and yellow warnings for drift or incomplete installation.
Synchronization is shown as unchecked when project validation fails. The summary
counts errors and warnings; unresolved issues still produce a failing exit status.
Enter an issue number to apply its available fix, `a` to apply all available fixes,
`r` to rerun diagnostics, or `q` to exit (then press Enter). Corrections are followed
by another check. Missing workspace locations retain their separate choice prompt.
Non-interactive runs never prompt, and `--json` keeps its existing output schema.

## Command logs

Every `rai` invocation writes a transcript outside the project. On Unix the
default is `~/.rai/logs/`; an explicit `XDG_CONFIG_HOME` uses
`$XDG_CONFIG_HOME/rai/logs/`. Windows uses `%APPDATA%\rai\logs\`.

Project transcripts live under `logs/projects/<project-name>-<path-sha256>/`.
The hash identifies the canonical absolute project path, so two projects with
the same name remain separate, while commands from a subdirectory or a symlink
to the same project share the same folder. Commands without a project, including
`install`, `uninstall`, `update`, `help`, `version`, and the watcher process,
go under `logs/global/`. Git and Codex hook invocations use their project folder.
Each watcher scan records a separate synchronization for each discovered project,
including failures and checks that require no changes. Commands selected in the
interactive helper receive their own transcript, linked to the helper invocation.

Each invocation has a unique UTC timestamp/PID filename ending in `.jsonl`.
Files contain one JSON object per line: a `start` event with arguments, project,
working directory, version, PID, and trigger; `output` events with a `stdout` or
`stderr` stream and text; and a `finish` event with `exitCode`, `durationMs`, and
an optional error. Output is written as it appears, without accumulating a
long-running watcher's output in memory. Concurrent commands use distinct files.
A process killed before completion may leave a transcript without a `finish`
event; its start and previously written output remain available.

Logging preserves terminal interaction and `--json` output. A logging failure
prints a warning on stderr and does not fail the command. On Unix, new log
directories use permissions `0700` and transcript files use `0600`. Existing
files or symbolic links in place of log directories are preserved and reported.
Transcripts retain the command arguments and displayed output, but do not record
stdin answers, hook input payloads, or the environment. Logs are retained until
you remove them; there is no automatic cleanup.

## What runs automatically

The optional per-user watcher polls configured workspace directories and searches all subdirectories. It stops descending once it finds `.agents/` and does not follow symbolic links. `rai install` can install a Git hook integration on the developer's machine before a clone. The `post-checkout` hook runs after the initial checkout of a normal clone, branch switch, or worktree creation. It checks for `.agents/` and calls the same sync operation used by the CLI. `post-merge` and `post-rewrite` cover merge-based pulls and rebases. The hook reports sync failures without making a successfully cloned repository appear to have failed.

`rai install` records each workspace's filesystem identity in `root-identities.json` next to `roots.txt`. If a configured path disappears, the watcher searches near its old location and under the user's home directory every five minutes; `rai doctor` searches immediately. A unique match updates `roots.txt`. If no match is found, `rai doctor` reports the missing workspace and keeps its entry so it can be restored or corrected. The search is bounded to 20,000 directories and nine levels per search root; moves outside those areas, moves across filesystems, inaccessible folders, and copies may need a new `rai install --root PATH` and removal of the stale entry. On Unix, identity uses device and inode; on Windows, it uses the volume and directory creation timestamp, and ambiguous matches are rejected. Existing installations gain identity records when their configured workspaces are next found at their saved paths.

Git has no native `pre-fetch` or `pre-pull` hook. A fetch by itself does not change the checked-out files. `git clone --no-checkout` does not run `post-checkout`; a later checkout or `rai sync` triggers synchronization. Git hooks are machine-local and are not supplied by the cloned repository. A repository-level `core.hooksPath` overrides the global path, so those repositories need their own integration or an explicit `rai sync`.

The watcher and hooks use one idempotent sync operation, so two triggers for the same change produce one effective update. Skills may help an agent explain RosettAI, but they are not required for synchronization. Developers should not need service or hook subcommands for ordinary use.

## Codex prompt guard

The generated `.codex/config.toml` declares a `UserPromptSubmit` command hook
that runs `rai sync --codex-hook`. When synchronization changes files, the hook
blocks the prompt and asks for a new Codex session. Project configuration and
the hook must be trusted by Codex before this guard runs. See [Codex workflow](codex.md).

## Generated projection edit guards

Generated Codex configuration registers a `PreToolUse` hook for `apply_patch`,
`Edit`, and `Write`; generated `.claude/settings.json` registers the same guard
for `Edit`, `Write`, and `MultiEdit`. They invoke the internal `rai guard` command,
which reads the harness event from stdin and denies edits, deletion, or moves to
active generated outputs. The message redirects instruction edits to
`.agents/rules/`, retaining a scoped rule's `path`; other resources belong under
`.agents/agents/`, `.agents/skills/`, or `.agents/mcp.yaml` as appropriate. Ordinary
project files and canonical files remain editable. The guard needs neither Git
nor a Codex executable and performs no synchronization or network access.
Running `rai guard` directly in a terminal exits immediately with an explanation
instead of waiting for hook input.

A digest mismatch identifies an edit after generation; it cannot identify its
author. Guard decisions identify intercepted agent tool attempts, not the author
of an existing changed file. The hooks must be enabled and trusted by the harness
and the installed `rai` must include the guard. Start a new session after updating
the generated configuration. Arbitrary shell, MCP, or specialized tool writes
are not comprehensively intercepted; these guards are not an OS sandbox and do
not prevent a person from editing files in their editor. Subsequent sync preserves
recognized edited projections in backups and restores canonical contents.

References: [Codex hooks](https://learn.chatgpt.com/docs/hooks) and
[Claude Code hooks](https://code.claude.com/docs/en/hooks).
