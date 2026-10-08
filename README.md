# RosettAI

RosettAI synchronizes canonical AI-agent resources from `.agents/` into native Codex, Claude Code, and GitHub Copilot Desktop project configuration.

The project is in an early proof-of-concept phase. The Rust CLI implements `install`, `uninstall`, `init`, `status`, `sync`, and `doctor` for rules, YAML subagents, skills, and MCP. Codex and Copilot read canonical skills in place. Directory-scoped rules are supported; native-file migration remains a design proposal.

## Product values

1. **Canonical over duplicated.** Define shared configuration once under `.agents/` instead of maintaining a separate copy for every harness.
2. **Explicit over magical.** Show what each adapter generates, how it transforms the source, and which parts a harness cannot support.
3. **Portable without pretending equivalence.** Preserve the team's intent across harnesses where their capabilities allow it, and report meaningful differences.
4. **Reproducible by default.** The same source configuration and adapter versions should produce the same output.
5. **Local and version controlled.** Commit the shared configuration with the repository and generate harness-specific configuration locally.

## Architecture

RosettAI's proposed model stores canonical resources in `.agents/`, validates them, and synchronizes local projections for supported installed harnesses. See the [current design](docs/doc.md) and [`rai` CLI contract](docs/cli.md). The [earlier architecture proposal](docs/architecture/README.md) remains available for background.

The [canonical source format](docs/doc.md#canonical-source-formats) is Markdown with YAML frontmatter for instructions and YAML for data-only configuration. The CLI also reads legacy YAML subagents and JSON MCP declarations during migration.

## Install the prebuilt CLI

On macOS, Linux, WSL, or Git Bash on Windows, run:

```sh
curl -fsSL https://rai.pival.fr | bash
```

The script downloads the latest stable release for your platform, verifies its
SHA-256 checksum, and installs `rai` in a user-owned directory. On Unix and WSL,
it reuses an existing `~/.cargo/bin/rai` or `~/.local/bin/rai` location;
otherwise it uses `~/.local/bin`. On Git Bash, it invokes the Windows PowerShell
installer below, which installs `rai.exe`. Set `RAI_INSTALL_DIR` to choose
another directory. No Rust toolchain or local build is required.
On Windows, run this in PowerShell:

```powershell
irm https://raw.githubusercontent.com/Gamma-Software/RosettAI/main/install.ps1 | iex
```

The PowerShell script downloads the prebuilt Windows ZIP, verifies its SHA-256
checksum, installs `rai.exe` in `%LOCALAPPDATA%\Programs\rai` by default, and
adds that directory to your user PATH. Set `RAI_INSTALL_DIR` to choose another
directory. Open a new terminal after installation. No Rust toolchain or local
build is required.

## Try the Rust proof of concept

Create Markdown rules in `.agents/rules/` of a test repository, then run:

```sh
cargo run -- sync --repo /path/to/test-repo --dry-run
cargo run -- sync --repo /path/to/test-repo
cargo test
```

The command generates Codex `AGENTS.md`/TOML files, Claude Code `CLAUDE.md`/`.claude`/`.mcp.json` files, and Copilot Desktop `.github` instruction/agent files plus `.vscode/mcp.json`, then adds exact output paths to the repository-root `.gitignore`. Rules stay directly in `.agents/rules/`; subagents use Markdown with YAML frontmatter in `.agents/subagents/`, and MCP declarations use `.agents/mcp.yaml`. See the [Codex workflow](docs/codex.md), [Claude Code adapter](docs/claude.md), and [Copilot Desktop workflow](docs/copilot-desktop.md). It preserves unowned, Git-tracked, or symlinked outputs. Modified active rai projections with an intact ownership marker are backed up outside the repository before regeneration. Generated Codex and Claude Code hooks block direct edits through supported editing tools and redirect agents to `.agents/`.

Run `cargo install --path .` to build and install `rai` from this checkout. Then `rai sync` creates an empty `.agents/` source tree on first manual use and synchronizes it. `rai status` previews drift, and `rai doctor` reports configuration problems. `rai install` sets up global Git hooks once for existing repositories and future clones. `rai install --root /path/to/workspace` additionally registers a directory for polling and installs a per-user watcher through launchd on macOS, systemd on Linux, or Task Scheduler on Windows. Setup is not run automatically by installation.

The sync report starts with its verdict and file counts, then groups full
repository-relative paths under Codex, Claude Code, GitHub Copilot, and Project
maintenance. Use `rai sync --compact` for a compact receipt that hides unchanged
file entries while keeping their count, diagnostics, comparisons, and backup
paths. Combine it with `--dry-run` to preview changes. `--compact` is only valid
with `sync`; it is accepted with hook modes and ignored with `--json`, whose
output stays unchanged. See the [sync report details](docs/cli.md#commands-a-developer-may-need).

Every normal command warns at the end when a newer stable GitHub Release is available, using a one-hour cache. The check requires `curl` and a network connection.

Run `rai update` to download and install the latest stable CLI release. It verifies the published SHA-256 checksum before replacing the executable; on Windows, replacement completes after the command exits.

In a terminal, `rai doctor` proposes a solution for each issue and lets you apply one safe fix or all available safe fixes. Unmanaged native files still require manual review; `--json` never prompts.

Add `--perf` to any command (for example, `rai sync --dry-run --perf`) to print only its elapsed time to stderr without changing `--json` output.

Every `rai` invocation saves its command, output, errors, duration, and exit code
under `~/.rai/logs/` by default. Project commands and automatic synchronizations
are grouped by project path; machine commands go under `logs/global/`. See the
[command logging details](docs/cli.md#command-logs) for paths and format.

## Releases

Pushing a version tag such as `v0.1.0` builds the CLI for Linux, macOS, and Windows and publishes the archives and checksums in a GitHub Release. The tag must match the version in `Cargo.toml`. See the [changelog](CHANGELOG.md) and the [CLI release skill](.agents/skills/release-cli/SKILL.md) for the preparation and verification steps.
