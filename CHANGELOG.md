# Changelog

Notable changes to the `rai` CLI are recorded here.

## Unreleased

## 0.4.2 - 2026-10-08

### Fixed

- Isolate Windows integration-test configuration and migration backups, preserve
  LF fixture contents, and compare native JSON paths portably.
- Add a checks-only release workflow mode to validate all five platforms on
  `main` before tagging. The v0.4.0 and v0.4.1 workflows stopped before publishing
  archives; v0.4.2 includes all changes listed below.

## 0.4.1 - 2026-10-08

### Fixed

- Make Windows workspace recovery tests deterministic when temporary directories
  share a creation timestamp, and verify that ambiguous matches preserve the saved
  configuration. The v0.4.0 release workflow stopped before publishing archives;
  v0.4.1 includes all changes listed below.

## 0.4.0 - 2026-10-08

### Added

- Sync reports grouped by Codex, Claude Code, GitHub Copilot, and project
  maintenance, with the verdict and file counters shown first.
- `rai sync --compact` to hide unchanged file entries while retaining their
  count, conflicts, comparisons, repair guidance, and backup paths.
- Content comparisons and per-file repair guidance for synchronization drift
  and conflicts, including a complete inventory when writes are blocked.
- External backups of edited active projections with intact ownership markers
  before regenerating them from canonical sources.
- Codex and Claude Code edit-tool guards that redirect changes to `.agents/`.
  These require trusted, enabled hooks and do not intercept arbitrary shell or
  MCP writes.
- Per-project and global command transcripts recording output, errors,
  duration, and exit status, including watcher synchronizations.
- `rai version`, `--version`, and `-V` with the version and build commit.
- Suggestions for mistyped public commands, with confirmation before running
  a suggested command in an interactive terminal.
- Scoped native instruction migration and rollback, with Git-tracking and
  `.gitignore` previews, verified backups, and resumable migration.
- Styled terminal help and grouped doctor diagnostics.
- Typed canonical resource directories for rules, agents, commands, and skills.
  Unsupported command projections are reported explicitly.
- `rai uninstall` to remove the per-user watcher and Git template.
- Watcher and `rai doctor` can recover a moved workspace by matching its saved
  filesystem identity near the previous location or in the user's home directory.
  For each unresolved workspace, interactive `rai doctor` asks whether to use a
  new location, stop watching it, or keep it for later.

### Changed

- Rename `rai setup` to `rai install` for machine integration.
- Remove regular `.keep` placeholders from populated canonical directories
  during successful synchronization; previews and blocked syncs preserve them.
- Display complete repository-relative sync paths consistently on all platforms.
- Store macOS and Linux per-user configuration under `~/.rai/` by default,
  migrating existing `~/.config/rai/` data and Git template references.
- Store the default Unix release-check cache under `~/.rai/cache/`.
- Record installed Git template hooks in a per-user JSON file and update it on
  uninstall.

## 0.3.1 - 2026-09-30

### Added

- Verified prebuilt installers for macOS, Linux, and Windows, including a Bash
  entry point on Windows.
- Automatic per-user watcher installation through systemd on Linux and Task
  Scheduler on Windows.

### Changed

- `rai doctor` detects a missing watcher service on Linux and Windows.
- Windows setup stores workspace roots in the user's `APPDATA` directory.

## 0.2.0 - 2026-09-30

### Added

- An automatic update warning at the end of CLI commands, with a one-hour cache
  and a short network timeout.
- `rai update` to install the latest stable platform release after SHA-256
  verification.

## 0.1.0 - 2026-09-30

### Added

- Initial Rust CLI with `setup`, `init`, `status`, `sync`, and `doctor` commands,
  plus a local polling watcher and Git hook integration.
- Synchronization of canonical rules, YAML subagents, skills, and MCP
  configuration from `.agents/` to Codex, Claude Code, and GitHub Copilot
  Desktop.
- Directory-scoped rules, dry-run and JSON output, performance timing, and a
  Codex prompt guard that requests a new session after synchronization.
- Validation and ownership checks that prevent replacement of unmanaged,
  modified, or Git-tracked generated files.
- Architecture, CLI, and harness workflow documentation.

### Known limitations

- Automatic migration of existing native harness files is not implemented.
- On platforms other than macOS, the watcher must currently be started
  manually with `rai watch`.
