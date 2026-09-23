# `rai` CLI contract

`rai` is the local command-line interface for RosettAI. This is a proposed interface for the first implementation, not a list of commands that already work. `.agents/` is the versioned source; generated harness configuration is local output.

## Common behavior

`rai` discovers the nearest repository containing `.agents/` by walking upward from the current directory. Repository commands accept `--repo <path>` to select a different checkout. `--harness <id>` limits a command to one detected harness and can be repeated; without it, all supported installed harnesses are included. Unknown IDs are errors. Read-only commands never change repository or machine state.

| Command | Purpose | Main options |
| --- | --- | --- |
| `rai sync` | Validate `.agents/`, calculate projections, update RosettAI-owned outputs, and maintain its block in the root `.gitignore`. | `--repo <path>`, repeatable `--harness <id>`, `--dry-run`, `--strict`, `--json` |
| `rai status` | Show detected harnesses, source and output revisions, unmanaged native files, drift, conflicts, and unsupported features. | `--repo <path>`, repeatable `--harness <id>`, `--json` |
| `rai doctor` | Check the source, harness discovery, ownership records, Git tracking, ignore rules, unmanaged native configuration, and service or hook health. | `--repo <path>`, `--json` |
| `rai migrate <path>` | Propose moving one unmanaged native instruction file into `.agents/rules/` while preserving its directory scope. | `--repo <path>`, `--dry-run` (default), `--apply` |
| `rai watch` | Run repository discovery and source-change monitoring in the foreground. | `--repo <path>` for one checkout, or repeatable `--root <path>` for directories containing checkouts |
| `rai service install` | Register the per-user background watcher to start at sign-in. | Repeatable `--root <path>` for discovery directories, `--start` to start immediately |
| `rai service status` | Show whether the watcher is installed and running, plus its last synchronization result. | `--json` |
| `rai service start`, `stop`, `uninstall` | Control or remove RosettAI's own per-user watcher registration. | None |
| `rai hooks install` | Add RosettAI's Git hook entry points to an existing checkout. | `--repo <path>` |
| `rai hooks status`, `uninstall` | Inspect or remove only RosettAI's hook entry points. | `--repo <path>`, `--json` for status |
| `rai version`, `rai help [command]` | Show the installed version or command help. | None |

`rai sync --dry-run` performs every read and validation step, then reports proposed file and `.gitignore` changes without writing. `--strict` fails if any requested feature cannot be represented by a selected harness; the default reports unsupported features and continues with compatible outputs. Neither mode overwrites an unowned or Git-tracked destination. `--json` produces a stable machine-readable result on stdout; human diagnostics go to stderr.

`rai migrate` accepts a repository-relative file such as `frontend/CLAUDE.md`. Its default output previews the canonical rule, scope, native files to be generated, removal of the original file, and any conflicts. `--apply` performs that reviewed move only after successful validation and synchronization; it keeps a local backup of an untracked original and never overwrites another unowned destination. If a native rule cannot be represented canonically, the proposal reports the limitation instead of dropping it.

At least one `--root` is required on the first service installation. Later `rai service install --root ...` calls replace the saved discovery roots. `rai watch` uses those saved roots when neither `--repo` nor `--root` is supplied; otherwise it asks for a scope.

## Git and automatic synchronization

`rai hooks install` preserves existing hook behavior and adds calls to `rai sync` after `post-checkout`, `post-merge`, and `post-rewrite`. It reports a conflict if it cannot compose with the checkout's existing hook configuration. Hooks installed in one checkout are not transferred by `git clone`; the per-user watcher discovers new clones. A fetch alone does not modify checked-out files, so it does not need a synchronization hook.

The watcher and hooks call the same idempotent sync operation. If both notice one change, the second run should detect that outputs are current and perform no writes. `rai service uninstall` removes only RosettAI's service registration; `rai hooks uninstall` removes only its hook entries. Neither command deletes `.agents/` or unowned harness configuration.

The root `.gitignore` is versioned. If `rai sync` adds a missing RosettAI block after a clone, Git will show that change until someone commits it; subsequent clones can reuse the committed rules. RosettAI must show this pending change in `rai status`.

## Results and examples

Exit codes: `0` means the command completed with no conflicts; `1` means validation, unsupported-feature (`--strict`), or ownership conflicts; `2` means invalid arguments or a missing repository; `3` means an operational failure such as an unwritable output. `status` and `doctor` use `1` when they find actionable drift or conflicts. JSON output includes a result code and per-harness diagnostics.

```sh
rai sync --dry-run
rai sync --harness claude-code --strict
rai status --json
rai doctor
rai migrate frontend/CLAUDE.md
rai service install --root /path/to/workspace --start
rai hooks install --repo /path/to/project
```

The harness ID list is adapter-defined and should be displayed by `rai help sync`; the example `claude-code` is illustrative. Migration formats and supported native locations are adapter-defined and must be shown in `rai help migrate`.
