---
name: release-cli
description: Prepare and publish a tagged release of the RosettAI `rai` CLI. Use when cutting a new CLI version, checking release readiness, or recovering a failed release workflow.
---

# Release the `rai` CLI

Read `Cargo.toml`, `CHANGELOG.md`, and `.github/workflows/release.yml` before acting. Treat the workflow as the source of truth if its contract changes. Inspect the working tree and recent tags; preserve unrelated changes.

## Prepare

1. Choose the next version from the changes since the previous release. Update `Cargo.toml` and refresh `Cargo.lock` with Cargo. Move shipped changes from `Unreleased` into a dated changelog section; leave `Unreleased` available for later changes.
2. Check that the binary still identifies itself as `rai` and that the version in `Cargo.toml`, changelog heading, and intended tag agree. Tags use `vX.Y.Z`; prereleases can use a suffix such as `v0.2.0-rc.1` and must match the Cargo package version without the `v`.
3. Run `cargo fmt --check`, `cargo test`, and `git diff --check`. Run any additional checks required by the release workflow. Resolve failures before tagging.
4. Review the exact release diff and commit the version and changelog update using the repository's Conventional Commit style. Do not include generated projections, local caches, or secrets.

## Publish

Create the release tag on the tested commit and push the commit and tag to `origin` when the user has authorized publishing. A pushed tag starts the release workflow. Do not move or reuse a published tag to retry a failed run; inspect the failed job, fix the cause, and use the recovery approach appropriate to the workflow and whether release assets were already published.

After the workflow finishes, verify its status, the GitHub release version and notes, and all five archives: Linux and macOS on x86_64 and aarch64, plus Windows on x86_64. Archives are named `rai-vX.Y.Z-<target>.tar.gz` except Windows, which uses `.zip`; verify `SHA256SUMS` too. A suffix marks the GitHub release as a prerelease. Report the commit, tag, release URL, checks performed, and any missing platform or failed job. Do not claim a release succeeded based only on a successful tag push.
