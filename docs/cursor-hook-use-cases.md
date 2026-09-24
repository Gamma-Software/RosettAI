# Cursor synchronization hook use cases

This document demonstrates the RosettAI `beforeSubmitPrompt` guard implemented
by `rai sync --cursor-hook`. The option is deliberately fail-closed: Cursor may call
the model only when RosettAI proves the generated projection is current.

## Guarantee

The integration guarantees this sequence:

```text
User submits a prompt
  -> Cursor runs beforeSubmitPrompt
  -> rai validates .agents and the generated projection
     -> current: allow the prompt
     -> stale: synchronize, block this submission, request resubmission
     -> unsafe or invalid: preserve existing files and block the prompt
  -> only an allowed submission reaches the Cursor backend
```

It does **not** claim that Cursor reloads a rule generated during the same hook
invocation. Cursor's documented hook output can allow or block a submission but
cannot inject context or request a rule reload. RosettAI therefore blocks the
first stale submission. The next submission starts only after the generated
`.cursor/rules/rosettai.mdc` exists and contains the canonical rules.

## Setup

Build and install the CLI:

```sh
cargo install --path .
```

Add this project hook to the target repository:

```json
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [
      {
        "command": "rai sync --cursor-hook",
        "timeout": 30
      }
    ]
  }
}
```

Cursor runs project hook commands from the repository root. The command drains
the JSON event supplied on stdin, discovers the nearest `.agents/` within the
Git repository, and writes exactly one Cursor-compatible JSON object to stdout.

## Use case 1: everything is already synchronized

Given this canonical rule:

```markdown
# .agents/rules/api.md

Use explicit error types for public APIs.
```

Run the initial projection:

```sh
rai sync
```

When a user submits a prompt, the hook evaluates the projection and returns:

```json
{"continue":true}
```

Result: Cursor may submit the prompt. The generated rule already existed before
the submission and is marked `alwaysApply: true` in
`.cursor/rules/rosettai.mdc`.

## Use case 2: a canonical rule changed seconds before the prompt

Start with a synchronized rule:

```markdown
Always use the old API.
```

Then change `.agents/rules/general.md` without manually running `rai sync`:

```markdown
Always use the new API.
```

On the first prompt submission, `rai sync --cursor-hook` detects both generated
projections as stale, updates them, and returns:

```json
{
  "continue": false,
  "user_message": "RosettAI synchronized CLAUDE.md, .cursor/rules/rosettai.mdc. Resubmit your prompt so Cursor resolves the updated rules before calling the model."
}
```

At this point the prompt has not reached the model, while the Cursor projection
contains:

```markdown
Always use the new API.
```

When the user resubmits the same prompt, the hook returns:

```json
{"continue":true}
```

Result: the allowed submission happens only after the updated rule is on disk.
This is the central safety property of the integration.

## Use case 3: an unmanaged Cursor rule occupies the output path

Suppose `.cursor/rules/rosettai.mdc` contains a hand-written rule without a
valid RosettAI ownership marker:

```markdown
User-owned rule. Do not replace this file.
```

The hook returns:

```json
{
  "continue": false,
  "user_message": "RosettAI synchronization is blocked: unowned or modified output conflict: .cursor/rules/rosettai.mdc"
}
```

Result:

- the prompt does not reach the model;
- the hand-written Cursor rule remains byte-for-byte unchanged;
- no partial `CLAUDE.md` projection is written;
- the user must migrate or move the conflicting file explicitly.

This verifies RosettAI's all-or-nothing planning and ownership boundary.

## Use case 4: the generated projection was edited manually

After a successful synchronization, suppose someone edits
`.cursor/rules/rosettai.mdc` directly. Its embedded content hash no longer
matches its body.

The next prompt is blocked with the same `unowned or modified output conflict`
diagnostic. RosettAI does not assume that a file is safe to replace merely
because it has the expected filename.

Result: manual changes are never silently discarded. The intended change must
be moved into `.agents/rules/` before synchronization can resume.

## Use case 5: canonical input is unsupported or invalid

The current proof of concept rejects scoped subdirectories such as:

```text
.agents/rules/
  backend/
    database.md
```

The hook returns `continue: false` with the validation error:

```text
scoped rules are not supported in this POC
```

No projection is created. The model cannot proceed with an incomplete or
silently flattened interpretation of the canonical configuration.

## Use case 6: `rai` cannot find canonical configuration

If the hook runs outside a repository containing `.agents/`, it returns a valid
blocking response rather than malformed output or a fail-open exit:

```json
{
  "continue": false,
  "user_message": "RosettAI cannot check this prompt: no .agents/ directory found inside ..."
}
```

This catches a misplaced hook configuration and prevents the absence of
canonical configuration from being mistaken for a synchronized state.

## Reproduce without opening Cursor

Cursor passes JSON on stdin. The hook currently needs only the repository
boundary, but it drains the payload to obey the hook protocol:

```sh
printf '%s' \
  '{"prompt":"Implement the API","attachments":[]}' \
  | rai sync --cursor-hook
```

This command produces exactly the JSON Cursor consumes, making all state and
failure paths testable without making an LLM request.

## Automated evidence

The integration tests exercise the compiled `rai` binary as an external process
and cover:

1. an already synchronized prompt is allowed;
2. drift synchronizes, blocks once, and allows resubmission;
3. the new canonical text is present and the old text absent before allowance;
4. unmanaged output is preserved and prevents partial writes;
5. invalid canonical input blocks without creating a projection.

Run them with:

```sh
cargo test cursor_hook
```

Run the complete project suite with:

```sh
cargo test
```

These tests prove RosettAI's side of the boundary: validation, synchronization,
ownership, hook JSON, and the block/resubmit sequence. Cursor's own documented
behavior establishes that `continue: false` prevents prompt submission. The
test suite intentionally makes no unverifiable claim about Cursor's internal
rule cache for a request already in progress.
