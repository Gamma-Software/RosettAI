# Harness sync integration coverage

Run `cargo test --test sync_scenarios` for the end-to-end decision matrix under `fixtures/sync-scenarios/`. Each case copies an input tree into a temporary repository and runs the built CLI with scripted answers. It covers empty setup, migration approval and refusal, the separate synchronize choice, existing canonical resources, unsupported and tracked native files, non-interactive hooks, read-only modes, symlinks, recursive instruction migration with directory scope preservation, later migration batches, rollback, discovery boundaries, named rule metadata, metadata validation across sync, status, doctor and hooks, validation before migration, source renaming with stable scope, and repeated synchronization. Projection assertions include all supported adapters (Codex, Claude Code, and GitHub Copilot) regardless of which harnesses are installed on the test machine.

Run `cargo test --test codex_sync` from the repository root. The example under `fixtures/codex-sync/input/` is a fresh project containing only `.agents/`. `fixtures/codex-sync/expected/` shows its complete tree after synchronization, including unchanged canonical files and generated Codex files. The one naming exception is `.gitignore.expected`: its contents are the generated `.gitignore`, but keeping the expected file under that name prevents Git from ignoring the other expected projections. The snapshot test copies the input to a temporary directory, runs the built `rai` binary, and compares every resulting file byte-for-byte with `expected/`.

Run `cargo test --test copilot_sync` for the dedicated GitHub Copilot Desktop adapter suite. It verifies global and `applyTo`-scoped instructions, custom agents, HTTP and STDIO MCP projection, omission of Codex-only agent fields, skills read in place, idempotence, cleanup, and collision handling.

The other tests invoke the built `rai` binary against isolated temporary repositories. Most require a Codex CLI on `PATH` at version 0.152.1 or newer; the version-gate test injects a local stub instead. They do not use network access or a real Codex session.

The suite covers:

- Dry-run, full projection, status, and idempotence.
- Root, scoped-only, nested, and multiple rules targeting one directory.
- Markdown frontmatter removal from generated `AGENTS.md` files.
- Canonical YAML subagent validation and Codex TOML projection.
- HTTP/STDIO MCP, custom-agent rejection, skills read in place, and prompt-hook configuration.
- Rule moves and agent removal, including cleanup of owned projections and `.gitignore` entries.
- Unowned, modified, Git-tracked, and symlink conflicts.
- Invalid canonical resources and unsupported Codex versions without partial writes.
- Prompt guard blocking on drift and allowing a clean next prompt.

The separate `tests/cli.rs` suite covers machine setup and the Git clone hook. These
offline suites do not start an actual Codex model session.

For a real runtime check against the locally installed and authenticated Codex CLI,
install the current `rai` binary and run:

```sh
cargo install --path .
tests/runtime/codex-runtime-smoke.sh
```

The runtime smoke test creates a temporary Git repository, runs `rai sync`, starts
`codex exec --json`, and verifies deterministic markers from projected `AGENTS.md`,
a canonical Skill, and a real MCP tool call. It is kept out
of `cargo test` because it consumes a live Codex request. Set
`KEEP_RUNTIME_FIXTURE=1` to retain the temporary repository and JSONL trace.

### Typed canonical resources

The `typed-resources` fixture covers `rules/` (`type: rule`), `agents/` (`type: agent`), and empty `commands/`. Tests exercise all adapters, scope preservation, idempotence, invalid and duplicate types, legacy `subagents/`, duplicate agent identities across both folders, and refusal of unsupported command projections before writes. Legacy rule metadata remains supported.
