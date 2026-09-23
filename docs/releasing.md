# Releasing Ragtag

`Cargo.toml` package metadata is the single source for the Ragtag release
version. GitHub Actions derives the tag as `v<resolved-version>`; the release
workflow has no independent version input.

## Version Migration

The manifest and lockfile now use `1.0.5`, the first unused version after the
existing `v1.0.4` release. The push that introduces this version is intended to
create the first hardened automated draft.

For later releases, update the `version` belonging to the `ragtag` package in
`Cargo.toml` and commit the corresponding `Cargo.lock` update. Cargo workspace
inheritance remains supported: if the package later uses
`version.workspace = true`, update the inherited workspace package version.
Never reuse a published version or delete, move, or force-update its tag.

## Build and Provenance Contract

On a push to `master`, the workflow resolves the package version at both push
revisions with `cargo metadata`. A root `Cargo.toml` edit that does not change
the resolved version is a successful no-op. A missing nonzero base revision,
such as after an unsafe force push, fails closed.

Every actual release attempt, including a manual recovery run or a rerun for an
existing release, performs all of these operations:

1. Rebuilds the six Linux, macOS, and Windows archives from the workflow commit.
2. Retains SHA-256 digests from each build job in separate workflow artifacts.
3. Generates GitHub artifact attestations for all six archives and all six
   checksum files with the official, commit-pinned `actions/attest` action.
4. Verifies local files against the retained build digests and verifies each
   attestation against this repository, `.github/workflows/release.yml`, and
   the exact workflow commit.
5. Revalidates the tag target immediately before every tag or release mutation.
6. Downloads the final remote assets and verifies their exact names, bytes,
   checksum pairing, retained digests, and commit-bound attestations.

The `.sha256` files remain convenient consumer checksums, but they are not
treated as an independent trust anchor. A replaced archive and replaced
checksum still fail because the remote bytes must match separately retained
build-job digests and a GitHub attestation for the workflow commit.

The `aarch64-pc-windows-msvc` package is cross-compiled and cannot be executed
on its hosted x86-64 runner. Review this limitation before publication.

## Create, Repair, and Rerun Behavior

The automatic release workflow only creates or updates **draft** releases.

* A new version receives a lightweight tag at the exact workflow commit before
  its draft is created. Existing tags are never updated.
* A partial draft is repairable only when its existing archive/checksum pairs
  already match retained build digests and valid attestations. Missing pairs
  are uploaded, and managed title or prerelease metadata is repaired.
* A complete matching draft is a verified no-op. The workflow still rebuilds,
  downloads every remote asset, compares bytes, and verifies attestations.
* A matching published release is also verified without mutation.
* Conflicting tags, duplicate or unexpected assets, orphaned checksums,
  substituted bytes, invalid provenance, incomplete published releases, and
  releases without resolvable tags fail closed.

The manual **Run workflow** action on the `Release` workflow is a draft recovery
operation for the selected `master` commit. It has no version input and
therefore cannot bypass Cargo metadata.

## Required Repository Rules

The workflow can detect a conflicting or moved tag and refuses to retarget it,
but it cannot prevent an administrator or other credential from changing a tag
between workflow runs. Configure a repository ruleset for release tags matching
`v*` with:

* tag updates and deletions blocked;
* force pushes blocked;
* creation restricted to the release workflow or trusted maintainers as
  appropriate for the repository;
* ruleset bypass limited to emergency administrators and audited.

Also protect `master` with required CI checks and restrict force pushes. These
rules preserve the commit identity checked by provenance verification and make
release tags effectively immutable after creation.

Enable GitHub immutable releases if available, and restrict release-editing
permission and ruleset bypasses to the smallest maintainer group.

## Review and Publish

Release automation never publishes a release. It stops after creating,
repairing, and verifying the GitHub draft. A human publishes the draft manually
through GitHub after the workflow succeeds; that manual transition is outside
the workflow and is not claimed to be atomic with automated verification.

Before selecting **Publish release** in the GitHub Releases interface:

* Confirm the tag and draft target the intended `master` commit.
* Confirm the title is `ragtag v<version>` and prerelease state matches SemVer.
* Confirm there are six archives and six matching `.sha256` files.
* Review generated notes and the Windows ARM64 cross-compilation limitation.
* Confirm the successful workflow run verified the final remote draft assets.
