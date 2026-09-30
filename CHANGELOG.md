# Changelog

Notable changes to the `rai` CLI are recorded here.

## Unreleased

### Added

- A macOS and Linux installer script for verified prebuilt GitHub releases.

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
