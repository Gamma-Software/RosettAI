# Changelog

Notable changes to the `rai` CLI are recorded here.

## Unreleased

### Added

- `rai uninstall` to remove the per-user watcher and Git template.
- Renamed the `rai setup` command to `rai install`.
- Watcher and `rai doctor` can recover a moved workspace by matching its saved
  filesystem identity near the previous location or in the user's home directory.
  For each unresolved workspace, interactive `rai doctor` asks whether to use a
  new location, stop watching it, or keep it for later.

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
