# RosettAI

RosettAI synchronizes canonical AI-agent resources from `.agents/` into Codex's native project configuration. Other harnesses remain future research, not supported outputs.

The project is in an early proof-of-concept phase. The Rust CLI implements `setup`, `init`, `status`, `sync`, and `doctor` for Codex rules, YAML subagents, and MCP. Codex reads canonical skills in place. Directory-scoped rules are supported; native-file migration remains a design proposal.

## Product values

1. **Canonical over duplicated.** Define shared configuration once under `.agents/` instead of maintaining a separate copy for every harness.
2. **Explicit over magical.** Show what each adapter generates, how it transforms the source, and which parts a harness cannot support.
3. **Portable without pretending equivalence.** Preserve the team's intent across harnesses where their capabilities allow it, and report meaningful differences.
4. **Reproducible by default.** The same source configuration and adapter versions should produce the same output.
5. **Local and version controlled.** Commit the shared configuration with the repository and generate harness-specific configuration locally.

## Architecture

RosettAI's proposed model stores canonical resources in `.agents/`, validates them, and synchronizes local projections for supported installed harnesses. See the [current design](docs/doc.md) and [`rai` CLI contract](docs/cli.md). The [earlier architecture proposal](docs/architecture/README.md) remains available for background.

## Try the Rust proof of concept

Create Markdown rules in `.agents/rules/` of a test repository, then run:

```sh
cargo run -- sync --repo /path/to/test-repo --dry-run
cargo run -- sync --repo /path/to/test-repo
cargo test
```

The command generates root or directory-scoped `AGENTS.md` files, `.codex/config.toml`, and `.codex/agents/*.toml`, then adds exact output paths to the repository-root `.gitignore`. Rules stay directly in `.agents/rules/`; an optional `path: frontend` Markdown frontmatter field projects a rule to `frontend/AGENTS.md`. Subagents are authored as YAML in `.agents/subagents/`. See the [Codex workflow](docs/codex.md). It refuses to replace existing unowned, modified, or Git-tracked outputs. Native-file migration is not supported; use a test repository because existing native files cause a reported conflict rather than being imported.

Run `cargo install --path .` to put `rai` on your `PATH`. Then `rai init` creates a starter `.agents/rules/general.md`, `rai status` previews drift, and `rai doctor` reports configuration problems. `rai setup --root /path/to/workspace` registers a workspace for polling and, on macOS, installs a per-user launchd watcher. It also configures Git hooks for future clones when no global hook path or custom template directory is already configured. Setup is not run automatically by installation. On other platforms, run the internal `rai watch` process manually for now.

In a terminal, `rai doctor` proposes a solution for each issue and lets you apply one safe fix or all available safe fixes. Unmanaged native files still require manual review; `--json` never prompts.

Add `--perf` to any command (for example, `rai sync --dry-run --perf`) to print only its elapsed time to stderr without changing `--json` output.
