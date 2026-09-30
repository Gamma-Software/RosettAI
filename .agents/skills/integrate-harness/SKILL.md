---
name: integrate-harness
description: Integrate a new coding-agent harness into RosettAI's Rust synchronizer, from capability research through safe projections, tests, and documentation. Use when adding support for a new harness or extending an existing harness adapter.
---

# Integrate a harness into RosettAI

## Establish the contract

Read `README.md`, `docs/doc.md`, `docs/cli.md`, the relevant existing harness document, and the current code before editing. The implemented source of truth is `.agents/`; `docs/architecture/ARCHITECTURE.md` is an earlier proposal and its `.rosettai/` layout and `rai run` launcher are not the current CLI contract. Inspect the worktree first and preserve unrelated changes.

Consult the harness's current official documentation and, where practical, its installed CLI or desktop diagnostics. Record the supported versions and how it discovers project instructions, scoped rules, agents, skills, MCP servers, and hooks. Distinguish native support, a documented transformation, and unsupported behavior. Do not claim that a file is loaded merely because it was written to a plausible path.

## Implement the adapter

- Read canonical resources from `.agents/rules/`, `.agents/subagents/`, `.agents/skills/`, and `.agents/mcp.json` as applicable. Keep secrets as environment references; never copy secret values into a projection.
- Put harness-specific conversion in `src/<harness>.rs` or an appropriate existing module. Wire it into `src/main.rs` planning so `sync`, `sync --dry-run`, `status`, and `doctor` observe the same outputs and errors. Extend setup or discovery only if the harness needs it and its detection is deterministic.
- Validate canonical inputs and reject unsupported fields or behavior with a clear diagnostic. If a transformation loses meaning, report that loss rather than silently omitting it. Follow existing user-visible behavior unless the task explicitly changes the CLI contract.
- Plan all paths and contents before writing. Reuse the existing ownership markers, hash checks, tracked-file and symlink collision checks, stale-output cleanup, and exact-path `.gitignore` rules. Add new output locations to those protections; never overwrite or delete an unowned or modified native file. Avoid ignoring a whole harness directory when only specific files are generated.
- Prefer deterministic output ordering and idempotent synchronization. Keep canonical resources tracked and generated projections local.

## Prove the integration

Add focused integration coverage in `tests/` using temporary repositories and realistic `.agents/` fixtures. Verify resource projection, scoped rules where supported, unsupported features, dry run, repeated sync, stale owned-output cleanup, and collisions with unowned, modified, tracked, or symlinked paths. Check that a planning failure makes no partial writes. Add a real harness smoke check only when its installed runtime and credentials are available; keep network or account-consuming checks separate from the offline suite.

Update `README.md`, `docs/cli.md`, `docs/doc.md`, and a harness-specific document where their claims or examples change. State the supported version, actual discovery path, capability gaps, and how to inspect loaded configuration in the harness. Run `cargo fmt --check`, `cargo test`, and `git diff --check`; resolve failures caused by this change. Report any validation that could not run and why.
