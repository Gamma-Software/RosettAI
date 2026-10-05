---
name: test-rosettai
description: Create and run RosettAI Rust unit or integration tests, including scenario fixtures and the separate Docker test images. Use when adding test coverage, reproducing a sync decision, or validating a code change.
---

# Test RosettAI

Choose the smallest test that verifies the behavior. Put pure logic checks in `#[cfg(test)]` beside the Rust code. Put CLI, filesystem, migration, Git hook, and adapter behavior in `tests/` so the test runs the built `rai` binary against a temporary repository. Use fixtures under `tests/integration/fixtures/` when file contents or repository layout matter. The scenario matrix is `tests/sync_scenarios.rs` with inputs in `tests/integration/fixtures/sync-scenarios/`.

For an integration test, cover the observable decision and its side effects: exit status, prompt or JSON result, generated files, and preservation of unmanaged files. Script interactive answers through stdin. Keep Git configuration, HOME, XDG paths, and test repositories isolated in temporary directories. Assert that a refused or blocked operation leaves its sources unchanged. When checking projections, exercise all supported adapters without depending on which harness applications are installed on the host.

Run the relevant test locally while developing. Before reporting completion, run the appropriate Docker image from the repository root:

```sh
docker build -f tests/docker-images/Dockerfile.tests -t rosettai-tests:local .
docker run --rm rosettai-tests:local

docker build -f tests/docker-images/Dockerfile.unit -t rosettai-unit:local .
docker run --rm rosettai-unit:local
```

The first image runs only integration suites in `tests/`; the second runs only unit tests in `src/`. Rebuild after changing source or tests because the image copies the repository at build time. If adding a new top-level `tests/*.rs` suite, add its name to both the prebuild and run commands in `Dockerfile.tests`. The images provide a fixed `codex --version` response for the adapter gate; use `tests/docker-images/Dockerfile.codex` for live harness checks when those are actually required.

Report which image and test suite ran, the result, and any platform-specific behavior that Linux Docker could not exercise. See `tests/docker-images/README.md` for the image commands and scope.
