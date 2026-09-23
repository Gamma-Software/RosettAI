# RosettAI — Architecture v0

RosettAI is a **canonical, versioned configuration format** for coding-agent harnesses. A repository stores only portable intent; adapters generate harness-specific configuration in a local ephemeral workspace at launch time.

## Goals

- One repository-owned source of truth for instructions, skills, agents, MCP declarations, commands, policies, and hooks.
- No generated `.claude/`, `.opencode/`, or `.github/` configuration is committed.
- Detect the selected harness at runtime and materialize only its compatible projection.
- Preserve reproducibility, explain transformations, and fail safely when a source feature has no equivalent.
- Keep secrets out of Git; resolve them only from the host environment or an explicit local secret provider.

## Non-goals for v0

- Reimplementing each harness runtime.
- Assuming behavioral equivalence for hooks, permissions, or proprietary UI features.
- Silently dropping unsupported configuration.

## Canonical repository tree

```text
.rosettai/
  manifest.yaml                 # schema version, defaults, target policy
  instructions/
    repository.md               # shared repository-level guidance
    development.md              # optional scoped guidance
  skills/
    release-changelog/
      SKILL.md
      assets/
  agents/
    converter-dev.md
    converter-gitlab.md
    converter-jira.md
  commands/
    release.md
  mcp/
    servers.yaml                # declarative server definitions, no secrets
  hooks/
    policy.yaml                 # portable intent; adapters report support
  policies/
    execution.yaml              # trust, permissions, secret references
  overlays/
    claude-code.yaml            # optional, explicit harness-specific escape hatch
    opencode.yaml
    github-copilot.yaml
  schemas/
    manifest.schema.json        # vendored/pinned schema in a later implementation

.gitignore
```

Generated locations are ignored, not canonical:

```gitignore
# RosettAI local materialization
.rosettai/.runtime/
.claude/
.opencode/
.github/agents/
.github/copilot-instructions.md
```

> Existing source-owned files in these directories must be migrated to `.rosettai/` before ignoring them. RosettAI must never broadly overwrite unowned files.

## Runtime model

1. A launcher (`rai run <harness> [args...]`) receives an explicit harness, or identifies it from the invoked command / environment.
2. It locates `.rosettai/manifest.yaml` by walking upward from the working directory.
3. It validates the canonical configuration and resolves inheritance, profiles, and overlays.
4. The adapter compiles a harness projection into a content-addressed local cache, e.g. `.rosettai/.runtime/<hash>/<harness>/`.
5. It writes a provenance report (source resource → generated file, transformations, warnings) beside that cache.
6. It launches the requested harness with its supported configuration path / working directory.
7. Cache cleanup is safe: delete only RosettAI-owned paths and retain the last known-good projection.

## Local Guardian (per laptop)

RosettAI installs a **per-user Windows Guardian**, started at sign-in and running without elevation. Its role is configuration integrity—not intercepting agent processes.

### Responsibilities

1. Discover installed supported harnesses from deterministic locations (known executables, package-manager registrations, and supported configuration roots); never infer a harness from arbitrary running processes.
2. Install or repair a small, RosettAI-owned global bootstrap instruction for each detected harness. The bootstrap says that a repository may contain `.rosettai/`, asks the agent to run `rai status`, and requires a visible warning when canonical configuration exists but no compatible projection is active.
3. Check the bootstrap hash periodically and after harness installation/update. If it was edited or removed, report drift and repair only with the user-approved policy.
4. Maintain local state, logs, and an audit trail under `%LOCALAPPDATA%\RosettAI\`; no guardian state belongs in repositories.
5. Offer `rai guardian status`, `repair`, `pause`, and `uninstall`. All writes are limited to explicit RosettAI ownership markers.

### Lifecycle

```text
Windows sign-in
  -> RosettAI Guardian starts as the current user
  -> discover harnesses and their supported global instruction locations
  -> verify RosettAI bootstrap marker and hash
  -> install/repair when policy permits
  -> repeat on a bounded interval and react to installer/update notifications where available
```

The recommended deployment is a user-level startup registration (for example, Task Scheduler at logon), not a privileged Windows service. This avoids machine-wide permissions and keeps the configuration scoped to the current developer account.

### Bootstrap behavior

The bootstrap is intentionally short, generic, and non-secret. It must:

- detect `.rosettai/manifest.yaml` by walking upward from the active workspace;
- call `rai status --harness <current-harness>` when available;
- warn the user if canonical resources are newer or not projected for that harness;
- never overwrite repository files, auto-run untrusted hooks, or expose secrets;
- clearly identify the warning as RosettAI-generated.

Adapters define exactly where this bootstrap is supported. If a harness has no reliable global instruction mechanism (for example, a closed desktop integration), the Guardian records it as **unmanaged** rather than claiming coverage. This warning path complements—not replaces—the wrapper launcher: the wrapper remains the only reliable way to ensure configuration is materialized before process startup.

## Adapter contract

Every adapter must return:

- `files`: exact generated paths and contents;
- `capabilities`: supported canonical features;
- `transformations`: lossless and lossy conversions;
- `warnings`: unsupported or permission-sensitive behavior;
- `ownership`: only paths safe for RosettAI to replace.

Capability resolution is explicit:

| Canonical feature | Claude Code | OpenCode | GitHub Copilot |
|---|---:|---:|---:|
| Instructions | Native | Native | Native projection |
| Skills | Native | Native | Instructions projection |
| Agents | Native | Native | Native |
| MCP declarations | Adapter-specific | Adapter-specific | Adapter-specific |
| Hooks | Native / policy-gated | Varies | Usually unsupported |
| Permissions | Native / policy-gated | Varies | Partial / unsupported |

An unsupported feature fails in `strict` mode and yields a warning in `compatible` mode. No feature is silently omitted.

## Safety and ownership

- Canonical input is versioned under `.rosettai/`.
- Secrets are references only (`env:NAME`, `secret:provider/key`), never literal values in canonical files.
- Generated state is excluded from Git and is content-addressed.
- A lock/provenance file records exact sources, adapter version, template version, target harness version, and output hashes.
- Launch occurs only after a successful compilation; a failed compilation leaves the prior cache intact.
- `rai doctor` reports collisions, unsupported resources, unresolved secrets, and stale caches.

## Migration from the HarnessTap experiment

The current project configuration detected one OpenCode skill and three agents. RosettAI migration places them in:

```text
.rosettai/skills/update-changelog-from-tag/SKILL.md
.rosettai/agents/converter-dev.md
.rosettai/agents/converter-gitlab.md
.rosettai/agents/converter-jira.md
```

Then adapters generate each harness projection locally. Git tracks only `.rosettai/`; HarnessTap-generated `apm.lock.yaml` and harness folders are not part of the target model.

## Delivery phases

1. **Schema and validator** — manifest, resource discovery, ownership rules, dry-run report.
2. **Read-only compiler** — Claude Code, OpenCode, and GitHub Copilot adapters producing cache files and provenance.
3. **Launcher** — explicit `rai run`, then safe auto-detection for supported commands.
4. **Migration** — import existing harness files into canonical resources, with reviewable diffs.
5. **Local Guardian** — per-user Windows startup, harness discovery, global bootstrap ownership, drift reporting, and repair controls.
6. **Advanced policy** — MCP secret providers, hook trust, profiles, remote bundles, and team governance.
