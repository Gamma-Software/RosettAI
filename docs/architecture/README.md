# RosettAI

RosettAI is a harness-neutral source of truth for AI-agent configuration. Repositories version only canonical resources in `.rosettai/`; local adapters compile temporary, ignored projections for Claude Code, OpenCode, and GitHub Copilot.

## Goals

- Define instructions, skills, agents, MCP declarations, hooks, and policies once.
- Produce explicit, auditable projections per harness without committing generated files.
- Preserve unsupported features through strict failures or visible compatibility warnings—never silent loss.
- Keep secrets as references and generated state in local content-addressed caches.

## Non-goals

- Replacing harnesses or masking their native capabilities.
- Reliably intercepting arbitrary direct agent launches; `rosetta run` is the dependable pre-launch path.
- Pretending a closed or unsupported harness is managed.

## Architecture

- [Architecture](ARCHITECTURE.md) — canonical tree, compiler/adapters, ownership, and Local Guardian model.
- [Runtime flow](runtime-architecture.mmd) · [SVG](runtime-architecture.svg)
- [Compilation safety](compilation-safety.mmd) · [SVG](compilation-safety.svg)
- [Guardian flow](guardian-architecture.mmd) · [SVG](guardian-architecture.svg)

## Local Guardian

The proposed per-user Windows Guardian starts at sign-in, discovers supported harnesses from deterministic locations, and verifies RosettAI-owned global bootstrap instructions. It only repairs owned content, records local state under `%LOCALAPPDATA%\RosettAI`, and emits a visible warning when a workspace contains `.rosettai/` but the active harness projection is stale or absent.

## Status

**Architecture phase.** The repository currently documents the design and diagrams; no executable compiler, launcher, or Guardian has been implemented yet.

## Planned delivery

1. Schema, validator, and dry-run report.
2. Read-only compiler with Claude Code, OpenCode, and GitHub Copilot adapters.
3. Explicit `rosetta run` launcher.
4. Migration tooling for existing harness configuration.
5. Local Guardian for bootstrap verification and repair.
