# Releasing Ragtag

Ragtag's release workflow creates draft GitHub releases containing the same
platform packages published by the authoritative
[v1.0.4 release](https://github.com/cwshugg/ragtag/releases/tag/v1.0.4).
Automation never publishes a release; a human reviews and publishes the draft
through GitHub.

`Cargo.toml` is the single source of truth for the package version. The current
version is `1.0.5`.

## Triggers and Version Resolution

The workflow runs for:

* pushes to `master` that change root `Cargo.toml` or `Cargo.lock`;
* pushes of tags matching `v*`; and
* manual `workflow_dispatch` runs.

For branch pushes and manual runs, the workflow reads the root `[package]`
version and derives exactly `v<version>`. Manual runs have no version or tag
input. For tag pushes, the pushed tag must equal the derived Cargo tag.

The PowerShell resolver validates the tag and runs `gh release view`, which
finds draft releases as well as published releases. If that tag already has a
release, the workflow succeeds without building or changing it. Otherwise, the
prepare job creates a draft with generated notes targeting the exact workflow
commit.

## Platforms and Assets

The build matrix reproduces every target from the published v1.0.4 release:

| Target | Runner | Archive |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | `.tar.gz` |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | `.tar.gz` |
| `x86_64-apple-darwin` | `macos-15-intel` | `.tar.gz` |
| `aarch64-apple-darwin` | `macos-15` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-2025` | `.zip` |
| `aarch64-pc-windows-msvc` | `windows-11-arm` | `.zip` |

Each archive is named `ragtag-v<version>-<target>.<extension>` and contains the
target's `ragtag` executable (`ragtag.exe` on Windows), `README.md`, and
`LICENSE`. Its checksum is a separate
`ragtag-v<version>-<target>.sha256` asset. The checksum filename deliberately
does not append `.sha256` to the archive's full filename; this matches v1.0.4.

The packaging action uses `.tar.gz` on Unix targets and `.zip` on Windows,
generates the corresponding SHA-256 file, and uploads both directly to the
draft. Every matrix entry runs on a GitHub-hosted runner with the same CPU
architecture as its Rust target. In particular, Linux AArch64 uses
`ubuntu-24.04-arm`, so the packaging action builds with Cargo directly and
cannot enter its implicit `cross` installation/container path.

The runner labels above are standard labels documented by GitHub for public
repositories. Cross-platform hosted builds cannot be reproduced completely on
a typical single local development machine.

## Idempotency and Draft Review

The release lookup happens before draft creation. A first run creates the
empty draft, then all matrix jobs upload their archive and checksum. Any
existing release for the tag makes later runs a successful no-op; the workflow
does not verify or repair partial drafts. If a build fails after draft
creation, inspect and delete the incomplete draft before intentionally
retriggering.

Checksums support ordinary download integrity checks but are not an independent
provenance mechanism. The workflow does not publish, attest, or remotely
revalidate uploaded assets.

Before manually publishing, confirm:

* the draft and eventual tag target the intended `master` commit;
* all six archives and six matching `.sha256` assets are present;
* archive names and extensions match the table above;
* each archive contains the correct target executable, `README.md`, and
  `LICENSE`; and
* every checksum validates its corresponding archive.

## Retrigger Version 1.0.5

Do not rerun an older failed workflow run because GitHub reruns use that run's
original commit and workflow definition.

After this workflow reaches `master`:

1. Open **Actions**, select **Release**, and choose **Run workflow**.
2. Select the `master` branch.
3. Start the run. There is no version or tag input.

The workflow reads `1.0.5` from `Cargo.toml`, derives `v1.0.5`, and creates the
draft at the selected `master` commit if no release already exists.
