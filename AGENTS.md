# Repository Guidelines

## Project Structure & Module Organization

RosettAI is currently in the architecture phase; no executable compiler, launcher, or Guardian exists yet. Project documentation lives in `docs/architecture/`:

- `ARCHITECTURE.md` defines the canonical configuration model, adapters, runtime flow, and safety boundaries.
- `README.md` provides the project overview and delivery roadmap.
- `*.mmd` files are editable Mermaid diagram sources; matching `*.svg` files are rendered artifacts.

Future canonical examples should follow the proposed `.rosettai/` layout documented in `ARCHITECTURE.md`. Keep generated harness projections and local runtime caches out of version control.

## Build, Test, and Development Commands

There is no build system or automated test suite yet. For documentation changes, use lightweight checks:

```sh
git diff --check
npx --yes @mermaid-js/mermaid-cli -i docs/architecture/runtime-architecture.mmd -o docs/architecture/runtime-architecture.svg
```

`git diff --check` detects whitespace errors. The Mermaid command regenerates an SVG after changing its source; replace the filenames for other diagrams. Review both the Markdown rendering and generated SVG before submitting.

## Coding Style & Naming Conventions

Write concise Markdown with ATX headings (`## Heading`), short paragraphs, and fenced code blocks with language identifiers. Use two-space indentation for nested YAML examples and descriptive lowercase kebab-case filenames such as `compilation-safety.mmd`. Keep each Mermaid source and its same-basename SVG synchronized. Use RosettAI terminology consistently: *canonical resources*, *adapter*, *projection*, and *Local Guardian*.

## Testing Guidelines

Documentation review is the current validation process. Check links, commands, diagram labels, and agreement with the safety guarantees in `ARCHITECTURE.md`. When implementation begins, place tests beside the relevant package or in a top-level `tests/` directory, document the chosen framework here, and require tests for validation, ownership, and unsupported-feature behavior.

## Commit & Pull Request Guidelines

Follow the existing Conventional Commit style, for example `docs: add architecture documentation`. Prefer focused commits with imperative, lowercase summaries.

Pull requests should explain the intent, list affected documents, and call out architectural or security tradeoffs. Link related issues. Include rendered previews or screenshots when diagrams change, and commit updated SVGs with their Mermaid sources.

## Security & Configuration

Never commit secrets or generated harness configuration. Examples must use references such as `env:NAME` or `secret:provider/key`. Preserve explicit ownership boundaries: RosettAI may replace only files it created and marked as owned.
