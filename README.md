# RosettAI

RosettAI helps teams share AI coding agent configuration without requiring every developer to use the same harness. MCP and Skills provide some common ground, but rules, plugins, hooks, and other harness settings still differ between tools. RosettAI aims to let a team define its intent once, then generate configuration for each supported harness and explain any differences.

The project is in an early proof-of-concept phase. The Rust CLI implements `setup`, `init`, `status`, `sync`, and `doctor` for global Markdown rules. Scoped rules, native-file migration, and harness detection remain design proposals.

## Product values

1. **Canonical over duplicated.** Define shared configuration once under `.agents/` instead of maintaining a separate copy for every harness.
2. **Explicit over magical.** Show what each adapter generates, how it transforms the source, and which parts a harness cannot support.
3. **Portable without pretending equivalence.** Preserve the team's intent across harnesses where their capabilities allow it, and report meaningful differences.
4. **Reproducible by default.** The same source configuration and adapter versions should produce the same output.
5. **Local and version controlled.** Commit the shared configuration with the repository and generate harness-specific configuration locally.

## Architecture

RosettAI's proposed model stores canonical resources in `.agents/`, validates them, and synchronizes local projections for supported installed harnesses. See the [current design](docs/doc.md) and [`rai` CLI contract](docs/cli.md). The [earlier architecture proposal](docs/architecture/README.md) remains available for background.

## Try the Rust proof of concept

Create one or more global Markdown rules in `.agents/rules/` of a test repository, then run:

```sh
cargo run -- sync --repo /path/to/test-repo --dry-run
cargo run -- sync --repo /path/to/test-repo
cargo test
```

The command generates `CLAUDE.md` and `.cursor/rules/rosettai.mdc`, and adds exact output paths to the repository-root `.gitignore`. It refuses to replace existing unowned, modified, or Git-tracked outputs. This POC does not yet detect installed harnesses: it always projects to both formats. It also does not support scoped rules or native-file migration. Use a test repository; existing native files will cause a reported conflict rather than being imported.

Run `cargo install --path .` to put `rai` on your `PATH`. Then `rai init` creates a starter `.agents/rules/general.md`, `rai status` previews drift, and `rai doctor` reports configuration problems. `rai setup --root /path/to/workspace` registers a workspace for polling and, on macOS, installs a per-user launchd watcher. It also configures Git hooks for future clones when no global hook path or custom template directory is already configured. Setup is not run automatically by installation. On other platforms, run the internal `rai watch` process manually for now.

In a terminal, `rai doctor` proposes a solution for each issue and lets you apply one safe fix or all available safe fixes. Unmanaged native files still require manual review; `--json` never prompts.

Add `--perf` to any command (for example, `rai sync --dry-run --perf`) to print only its elapsed time to stderr without changing `--json` output.
