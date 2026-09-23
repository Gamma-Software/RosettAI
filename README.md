# RosettAI

RosettAI helps teams share AI coding agent configuration without requiring every developer to use the same harness. MCP and Skills provide some common ground, but rules, plugins, hooks, and other harness settings still differ between tools. RosettAI aims to let a team define its intent once, then generate configuration for each supported harness and explain any differences.

The project is in the architecture phase. The CLI, local synchronizer, and adapters described below have not been implemented yet.

## Product values

1. **Canonical over duplicated.** Define shared configuration once under `.agents/` instead of maintaining a separate copy for every harness.
2. **Explicit over magical.** Show what each adapter generates, how it transforms the source, and which parts a harness cannot support.
3. **Portable without pretending equivalence.** Preserve the team's intent across harnesses where their capabilities allow it, and report meaningful differences.
4. **Reproducible by default.** The same source configuration and adapter versions should produce the same output.
5. **Local and version controlled.** Commit the shared configuration with the repository and generate harness-specific configuration locally.

## Architecture

RosettAI's proposed model stores canonical resources in `.agents/`, validates them, and synchronizes local projections for supported installed harnesses. See the [current design](docs/doc.md) and [`rai` CLI contract](docs/cli.md). The [earlier architecture proposal](docs/architecture/README.md) remains available for background.
