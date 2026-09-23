#!/usr/bin/env python3
"""Resolve, create, and verify Cargo-driven draft releases."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from typing import Any
from urllib import error, parse, request

PACKAGE_NAME = "ragtag"
ZERO_SHA = "0" * 40
SHA_PATTERN = re.compile(r"^[0-9a-fA-F]{40}$")
DIGEST_PATTERN = re.compile(r"^[0-9a-f]{64}$")
API_VERSION = "2022-11-28"
SIGNER_WORKFLOW = ".github/workflows/release.yml"
RELEASE_BRANCH = "master"
PLATFORMS = (
    {
        "target": "x86_64-unknown-linux-gnu",
        "os": "ubuntu-latest",
        "extension": "tar.gz",
    },
    {
        "target": "aarch64-unknown-linux-gnu",
        "os": "ubuntu-latest",
        "extension": "tar.gz",
    },
    {
        "target": "x86_64-apple-darwin",
        "os": "macos-latest",
        "extension": "tar.gz",
    },
    {
        "target": "aarch64-apple-darwin",
        "os": "macos-latest",
        "extension": "tar.gz",
    },
    {
        "target": "x86_64-pc-windows-msvc",
        "os": "windows-latest",
        "extension": "zip",
    },
    {
        "target": "aarch64-pc-windows-msvc",
        "os": "windows-latest",
        "extension": "zip",
    },
)


class ReleaseMetadataError(RuntimeError):
    """Report an unsafe or invalid release state."""


def run_command(
    arguments: list[str],
    cwd: Path | None = None,
) -> subprocess.CompletedProcess[bytes]:
    """Run a command and retain output for contextual error reporting."""
    return subprocess.run(
        arguments,
        cwd=cwd,
        check=False,
        capture_output=True,
    )


def validate_sha(value: str, label: str) -> str:
    """Require a full hexadecimal Git object ID."""
    if not SHA_PATTERN.fullmatch(value):
        raise ReleaseMetadataError(
            f"{label} must be a full 40-character hexadecimal Git SHA"
        )
    return value.lower()


def extract_revision(repository: Path, revision: str, destination: Path) -> None:
    """Materialize one trusted Git revision without changing the working tree."""
    archive = run_command(
        ["git", "archive", "--format=tar", revision],
        repository,
    )
    if archive.returncode != 0:
        detail = archive.stderr.decode(errors="replace").strip()
        raise ReleaseMetadataError(
            f"Git revision {revision} is unavailable; refusing to infer a "
            f"release across a missing or force-pushed base: {detail}"
        )

    try:
        with tarfile.open(fileobj=io.BytesIO(archive.stdout), mode="r:") as bundle:
            bundle.extractall(destination, filter="data")
    except (tarfile.TarError, OSError) as exc:
        raise ReleaseMetadataError(
            f"could not materialize Git revision {revision}: {exc}"
        ) from exc


def select_package_version(
    metadata: dict[str, Any],
    package_name: str,
) -> str | None:
    """Select exactly one package version by Cargo package name."""
    packages = [
        package
        for package in metadata.get("packages", [])
        if package.get("name") == package_name
    ]
    if not packages:
        return None
    if len(packages) != 1:
        raise ReleaseMetadataError(
            f"Cargo metadata contains {len(packages)} packages named "
            f"{package_name!r}; release identity is ambiguous"
        )

    version = packages[0].get("version")
    if not isinstance(version, str) or not version:
        raise ReleaseMetadataError(
            f"Cargo package {package_name!r} has no resolved version"
        )
    return version


def resolve_revision_version(
    repository: Path,
    revision: str,
    package_name: str = PACKAGE_NAME,
) -> str | None:
    """Resolve a package version at a Git revision through `cargo metadata`."""
    with tempfile.TemporaryDirectory(prefix="ragtag-release-") as temporary:
        source = Path(temporary)
        extract_revision(repository, revision, source)
        manifest = source / "Cargo.toml"
        if not manifest.is_file():
            return None

        result = run_command(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
                str(manifest),
            ],
            source,
        )
        if result.returncode != 0:
            detail = result.stderr.decode(errors="replace").strip()
            raise ReleaseMetadataError(
                f"cargo metadata failed at revision {revision}: {detail}"
            )

        try:
            metadata = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise ReleaseMetadataError(
                f"cargo metadata returned invalid JSON at revision {revision}"
            ) from exc
        return select_package_version(metadata, package_name)


def is_prerelease(version: str) -> bool:
    """Return whether a Cargo SemVer contains a pre-release component."""
    return "-" in version.partition("+")[0]


def release_matrix() -> dict[str, tuple[dict[str, str], ...]]:
    """Return the workflow matrix from the release platform contract."""
    return {"include": PLATFORMS}


def archive_name(version: str, target: str, extension: str) -> str:
    """Return one distributable archive name."""
    return f"ragtag-v{version}-{target}.{extension}"


def expected_archive_names(version: str) -> set[str]:
    """Return the six platform archive names for one Cargo version."""
    return {
        archive_name(version, platform["target"], platform["extension"])
        for platform in PLATFORMS
    }


def expected_asset_names(version: str) -> set[str]:
    """Return the complete release asset contract for one Cargo version."""
    archives = expected_archive_names(version)
    return archives | {f"{archive}.sha256" for archive in archives}


def detect_versions(
    repository: Path,
    event: str,
    head: str,
    before: str | None,
) -> dict[str, str]:
    """Resolve current metadata and compare it with a push's base revision."""
    head_sha = validate_sha(head, "head")
    current = resolve_revision_version(repository, head_sha)
    if current is None:
        raise ReleaseMetadataError(
            f"revision {head_sha} does not contain package {PACKAGE_NAME!r}"
        )

    old_version: str | None = None
    if event == "push":
        if before is None:
            raise ReleaseMetadataError("push events require a before SHA")
        before_sha = validate_sha(before, "before")
        if before_sha != ZERO_SHA:
            old_version = resolve_revision_version(repository, before_sha)

    changed = event == "workflow_dispatch" or old_version != current
    return {
        "old_version": old_version or "",
        "version": current,
        "tag": f"v{current}",
        "prerelease": str(is_prerelease(current)).lower(),
        "changed": str(changed).lower(),
        "matrix": json.dumps(release_matrix(), separators=(",", ":")),
    }


def github_get(
    repository: str,
    endpoint: str,
    token: str,
    missing_statuses: tuple[int, ...] = (404,),
) -> dict[str, Any] | None:
    """Read a GitHub API object, recognizing endpoint-specific missing states."""
    url = f"https://api.github.com/repos/{repository}/{endpoint}"
    api_request = request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "ragtag-release-workflow",
            "X-GitHub-Api-Version": API_VERSION,
        },
    )
    try:
        with request.urlopen(api_request, timeout=30) as response:
            return json.load(response)
    except error.HTTPError as exc:
        if exc.code in missing_statuses:
            return None
        detail = exc.read().decode(errors="replace").strip()
        raise ReleaseMetadataError(
            f"GitHub API request failed with HTTP {exc.code}: {detail}"
        ) from exc
    except (error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        raise ReleaseMetadataError(f"GitHub API request failed: {exc}") from exc


def asset_names(release: dict[str, Any]) -> set[str]:
    """Read unique, well-formed release asset names."""
    names: list[str] = []
    for asset in release.get("assets", []):
        name = asset.get("name")
        if not isinstance(name, str) or not name:
            raise ReleaseMetadataError("release contains an unnamed asset")
        names.append(name)
    if len(names) != len(set(names)):
        raise ReleaseMetadataError("release contains duplicate asset names")
    return set(names)


def classify_remote_state(
    expected_sha: str,
    expected_assets: set[str],
    tag_sha: str | None,
    release: dict[str, Any] | None,
) -> str:
    """Reject conflicts and schedule builds for every release validation."""
    if tag_sha is not None and tag_sha.lower() != expected_sha.lower():
        raise ReleaseMetadataError(
            f"existing tag targets {tag_sha}, not release commit {expected_sha}"
        )
    if release is None:
        return "build"
    if tag_sha is None:
        raise ReleaseMetadataError(
            "a release exists without a resolvable tag; refusing to repair it"
        )

    unexpected = asset_names(release) - expected_assets
    if unexpected:
        names = ", ".join(sorted(unexpected))
        raise ReleaseMetadataError(f"release contains unexpected assets: {names}")
    return "build"


def plan_remote_release(
    repository: str,
    tag: str,
    version: str,
    sha: str,
    token: str,
) -> str:
    """Preflight tag and release state before scheduling builds."""
    encoded_tag = parse.quote(tag, safe="")
    commit = github_get(
        repository,
        f"commits/{encoded_tag}",
        token,
        missing_statuses=(404, 422),
    )
    tag_sha = None if commit is None else str(commit.get("sha", ""))
    if tag_sha == "":
        raise ReleaseMetadataError(f"GitHub returned no commit SHA for tag {tag}")

    release = github_get(repository, f"releases/tags/{encoded_tag}", token)
    return classify_remote_state(
        sha,
        expected_asset_names(version),
        tag_sha,
        release,
    )


def sha256_file(path: Path) -> str:
    """Compute a file's SHA-256 digest."""
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def platform_for_target(target: str) -> dict[str, str]:
    """Resolve exactly one supported release target."""
    matches = [platform for platform in PLATFORMS if platform["target"] == target]
    if len(matches) != 1:
        raise ReleaseMetadataError(f"unsupported release target: {target}")
    return matches[0]


def write_build_manifest(
    directory: Path,
    target: str,
    version: str,
    output: Path,
) -> None:
    """Retain independent digests for one build job's release files."""
    platform = platform_for_target(target)
    archive = archive_name(version, target, platform["extension"])
    names = (archive, f"{archive}.sha256")
    missing = [name for name in names if not (directory / name).is_file()]
    if missing:
        raise ReleaseMetadataError(
            f"build output is missing expected files: {', '.join(missing)}"
        )
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(
            {
                "target": target,
                "files": {name: sha256_file(directory / name) for name in names},
            },
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def load_build_manifests(directory: Path, version: str) -> dict[str, str]:
    """Load one independent build digest manifest per supported target."""
    paths = sorted(directory.glob("*.json"))
    if len(paths) != len(PLATFORMS):
        raise ReleaseMetadataError(
            f"expected {len(PLATFORMS)} build manifests, found {len(paths)}"
        )

    targets: set[str] = set()
    digests: dict[str, str] = {}
    for path in paths:
        try:
            manifest = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise ReleaseMetadataError(
                f"could not read build manifest {path.name}: {exc}"
            ) from exc
        target = manifest.get("target")
        if not isinstance(target, str) or target in targets:
            raise ReleaseMetadataError(
                f"build manifest {path.name} has a duplicate or invalid target"
            )
        platform_for_target(target)
        targets.add(target)
        files = manifest.get("files")
        if not isinstance(files, dict):
            raise ReleaseMetadataError(
                f"build manifest {path.name} has no file digest map"
            )
        for name, digest in files.items():
            if (
                not isinstance(name, str)
                or not isinstance(digest, str)
                or not DIGEST_PATTERN.fullmatch(digest)
                or name in digests
            ):
                raise ReleaseMetadataError(
                    f"build manifest {path.name} contains an invalid file digest"
                )
            digests[name] = digest

    expected_targets = {platform["target"] for platform in PLATFORMS}
    if targets != expected_targets:
        raise ReleaseMetadataError("build manifests do not cover the release matrix")
    if set(digests) != expected_asset_names(version):
        raise ReleaseMetadataError(
            "build manifests do not describe the complete release asset set"
        )
    return digests


def validate_checksum_pair(directory: Path, archive_name_value: str) -> None:
    """Validate a co-located checksum as an additional consistency check."""
    checksum_path = directory / f"{archive_name_value}.sha256"
    fields = checksum_path.read_text(encoding="utf-8").strip().split()
    if len(fields) != 2:
        raise ReleaseMetadataError(
            f"{checksum_path.name} does not contain one SHA-256 record"
        )
    digest, recorded_name = fields
    if recorded_name.lstrip("*") != archive_name_value:
        raise ReleaseMetadataError(
            f"{checksum_path.name} names {recorded_name}, not {archive_name_value}"
        )
    if digest.lower() != sha256_file(directory / archive_name_value):
        raise ReleaseMetadataError(
            f"{checksum_path.name} does not match {archive_name_value}"
        )


def validate_files(
    directory: Path,
    expected_digests: dict[str, str],
    expected_names: set[str] | None = None,
) -> None:
    """Validate an exact file set against independently retained digests."""
    names = set(expected_digests) if expected_names is None else expected_names
    actual = {path.name for path in directory.iterdir() if path.is_file()}
    if actual != names:
        raise ReleaseMetadataError(
            "release files do not match the expected asset set: "
            f"expected {sorted(names)}, found {sorted(actual)}"
        )
    for name in sorted(names):
        expected = expected_digests.get(name)
        if expected is None or sha256_file(directory / name) != expected:
            raise ReleaseMetadataError(
                f"release file {name} does not match its retained build digest"
            )

    archives = {name for name in names if not name.endswith(".sha256")}
    checksums = {
        name.removesuffix(".sha256") for name in names if name.endswith(".sha256")
    }
    if archives != checksums:
        raise ReleaseMetadataError(
            "release state contains an archive without its checksum or vice versa"
        )
    for name in sorted(archives):
        validate_checksum_pair(directory, name)


class GitHubClient:
    """Perform authenticated GitHub release and attestation operations."""

    def __init__(self, repository: str, token: str) -> None:
        self.repository = repository
        self.token = token

    def _gh(self, arguments: list[str]) -> bytes:
        result = run_command(["gh", *arguments])
        if result.returncode != 0:
            detail = result.stderr.decode(errors="replace").strip()
            raise ReleaseMetadataError(
                f"GitHub CLI command failed: gh {' '.join(arguments)}: {detail}"
            )
        return result.stdout

    def tag_sha(self, tag: str) -> str | None:
        """Resolve a tag to a commit SHA without accepting ambiguous failures."""
        encoded_tag = parse.quote(tag, safe="")
        commit = github_get(
            self.repository,
            f"commits/{encoded_tag}",
            self.token,
            missing_statuses=(404, 422),
        )
        if commit is None:
            return None
        sha = commit.get("sha")
        if not isinstance(sha, str) or not sha:
            raise ReleaseMetadataError(f"GitHub returned no commit SHA for tag {tag}")
        return sha.lower()

    def release(self, tag: str) -> dict[str, Any] | None:
        """Read a release by tag."""
        encoded_tag = parse.quote(tag, safe="")
        return github_get(
            self.repository,
            f"releases/tags/{encoded_tag}",
            self.token,
        )

    def create_tag(self, tag: str, sha: str) -> None:
        """Create a lightweight tag without ever updating an existing ref."""
        self._gh(
            [
                "api",
                "--method",
                "POST",
                f"repos/{self.repository}/git/refs",
                "-f",
                f"ref=refs/tags/{tag}",
                "-f",
                f"sha={sha}",
            ]
        )

    def create_release(
        self,
        tag: str,
        title: str,
        prerelease: bool,
        files: list[Path],
    ) -> None:
        """Create a draft release for an already verified tag."""
        arguments = [
            "release",
            "create",
            tag,
            *(str(path) for path in files),
            "--draft",
            "--generate-notes",
            "--title",
            title,
            "--verify-tag",
        ]
        if prerelease:
            arguments.append("--prerelease")
        self._gh(arguments)

    def upload(self, tag: str, files: list[Path]) -> None:
        """Upload and replace managed assets on an existing draft."""
        self._gh(
            [
                "release",
                "upload",
                tag,
                *(str(path) for path in files),
                "--clobber",
            ]
        )

    def patch_release(
        self,
        release_id: int,
        title: str,
        prerelease: bool,
    ) -> None:
        """Repair mutable draft release metadata."""
        self._gh(
            [
                "api",
                "--method",
                "PATCH",
                f"repos/{self.repository}/releases/{release_id}",
                "-f",
                f"name={title}",
                "-F",
                "draft=true",
                "-F",
                f"prerelease={str(prerelease).lower()}",
            ]
        )

    def download(self, tag: str, destination: Path) -> None:
        """Download every managed release asset."""
        self._gh(
            [
                "release",
                "download",
                tag,
                "--dir",
                str(destination),
            ]
        )

    def verify_attestation(self, path: Path, sha: str) -> None:
        """Verify provenance from this workflow and exact source commit."""
        self._gh(
            [
                "attestation",
                "verify",
                str(path),
                "--repo",
                self.repository,
                "--signer-workflow",
                f"{self.repository}/{SIGNER_WORKFLOW}",
                "--source-digest",
                sha,
                "--source-ref",
                f"refs/heads/{RELEASE_BRANCH}",
                "--deny-self-hosted-runners",
            ]
        )


def assert_tag_target(
    client: GitHubClient,
    tag: str,
    sha: str,
    allow_missing: bool = False,
) -> None:
    """Fail unless a tag immediately resolves to the intended commit."""
    actual = client.tag_sha(tag)
    if actual is None and allow_missing:
        return
    if actual != sha:
        raise ReleaseMetadataError(
            f"tag {tag} targets {actual or 'nothing'}, not release commit {sha}"
        )


def verify_attestations(
    client: GitHubClient,
    directory: Path,
    names: set[str],
    sha: str,
) -> None:
    """Verify every release file's GitHub build provenance."""
    for name in sorted(names):
        client.verify_attestation(directory / name, sha)


def download_and_verify(
    client: GitHubClient,
    tag: str,
    expected_digests: dict[str, str],
    names: set[str],
    sha: str,
) -> None:
    """Download remote assets and verify bytes, checksums, and attestations."""
    with tempfile.TemporaryDirectory(prefix="ragtag-remote-release-") as temporary:
        destination = Path(temporary)
        client.download(tag, destination)
        validate_files(destination, expected_digests, names)
        verify_attestations(client, destination, names, sha)


def validate_release_shape(
    release: dict[str, Any],
    expected_assets: set[str],
) -> set[str]:
    """Reject unmanaged, duplicate, or impossible release asset states."""
    names = asset_names(release)
    unexpected = names - expected_assets
    if unexpected:
        raise ReleaseMetadataError(
            "release contains unexpected assets: " + ", ".join(sorted(unexpected))
        )
    archives = {name for name in names if not name.endswith(".sha256")}
    checksums = {
        name.removesuffix(".sha256") for name in names if name.endswith(".sha256")
    }
    if archives != checksums:
        raise ReleaseMetadataError(
            "release contains an archive without its checksum or vice versa"
        )
    return names


def release_metadata_matches(
    release: dict[str, Any],
    title: str,
    prerelease: bool,
) -> bool:
    """Return whether release metadata matches the managed contract."""
    return (
        release.get("name") == title
        and bool(release.get("prerelease", False)) == prerelease
    )


def ensure_draft_release(
    client: GitHubClient,
    directory: Path,
    manifests: Path,
    tag: str,
    version: str,
    sha: str,
    prerelease: bool,
) -> str:
    """Create, repair, or verify a draft release while failing closed."""
    release_sha = validate_sha(sha, "release")
    expected_digests = load_build_manifests(manifests, version)
    expected_assets = set(expected_digests)
    expected_title = f"ragtag {tag}"
    validate_files(directory, expected_digests)
    verify_attestations(client, directory, expected_assets, release_sha)

    tag_sha = client.tag_sha(tag)
    if tag_sha is not None and tag_sha != release_sha:
        raise ReleaseMetadataError(
            f"existing tag {tag} targets {tag_sha}, not {release_sha}"
        )
    release = client.release(tag)
    if release is not None and tag_sha is None:
        raise ReleaseMetadataError(
            "release exists without a resolvable tag; refusing to mutate it"
        )

    files = [directory / name for name in sorted(expected_assets)]
    action = "verified"
    if release is None:
        if tag_sha is None:
            assert_tag_target(client, tag, release_sha, allow_missing=True)
            if client.release(tag) is not None:
                raise ReleaseMetadataError(
                    "release appeared while creating its tag; refusing to continue"
                )
            client.create_tag(tag, release_sha)
        if client.release(tag) is not None:
            raise ReleaseMetadataError(
                "release appeared before creation; refusing to overwrite it"
            )
        assert_tag_target(client, tag, release_sha)
        client.create_release(
            tag,
            expected_title,
            prerelease,
            files,
        )
        action = "created"
    else:
        names = validate_release_shape(release, expected_assets)
        complete = names == expected_assets
        if complete:
            download_and_verify(
                client,
                tag,
                expected_digests,
                names,
                release_sha,
            )
        elif names:
            download_and_verify(
                client,
                tag,
                expected_digests,
                names,
                release_sha,
            )

        if not bool(release.get("draft", False)):
            if not complete or not release_metadata_matches(
                release,
                expected_title,
                prerelease,
            ):
                raise ReleaseMetadataError(
                    "published release does not match the managed contract"
                )
        else:
            if not complete:
                assert_tag_target(client, tag, release_sha)
                client.upload(tag, files)
                action = "repaired"
            if not release_metadata_matches(
                release,
                expected_title,
                prerelease,
            ):
                release_id = release.get("id")
                if not isinstance(release_id, int):
                    raise ReleaseMetadataError(
                        "draft release has no numeric GitHub release ID"
                    )
                assert_tag_target(client, tag, release_sha)
                client.patch_release(
                    release_id,
                    expected_title,
                    prerelease,
                )
                action = "repaired"

    assert_tag_target(client, tag, release_sha)
    final_release = client.release(tag)
    if final_release is None:
        raise ReleaseMetadataError("release disappeared after validation")
    final_names = validate_release_shape(final_release, expected_assets)
    if final_names != expected_assets:
        raise ReleaseMetadataError("release does not contain the complete asset set")
    if bool(final_release.get("draft", False)):
        if not release_metadata_matches(
            final_release,
            expected_title,
            prerelease,
        ):
            raise ReleaseMetadataError("draft release metadata is incorrect")
    elif action != "verified":
        raise ReleaseMetadataError("workflow mutation unexpectedly published a release")
    download_and_verify(
        client,
        tag,
        expected_digests,
        final_names,
        release_sha,
    )
    return action


def write_outputs(path: Path, values: dict[str, str]) -> None:
    """Append simple validated values to a GitHub Actions output file."""
    with path.open("a", encoding="utf-8") as output:
        for name, value in values.items():
            if "\n" in value or "\r" in value:
                raise ReleaseMetadataError(
                    f"output {name!r} contains an unexpected newline"
                )
            output.write(f"{name}={value}\n")


def add_detect_parser(subparsers: Any) -> None:
    """Add the release-detection command."""
    parser = subparsers.add_parser("detect")
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument(
        "--event",
        choices=("push", "workflow_dispatch"),
        required=True,
    )
    parser.add_argument("--head", required=True)
    parser.add_argument("--before")
    parser.add_argument("--github-output", type=Path, required=True)
    parser.add_argument("--github-repository")


def add_manifest_parser(subparsers: Any) -> None:
    """Add the build-manifest command."""
    parser = subparsers.add_parser("manifest")
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)


def add_ensure_draft_parser(subparsers: Any) -> None:
    """Add the draft creation and verification command."""
    parser = subparsers.add_parser("ensure-draft")
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--manifests", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--prerelease", choices=("true", "false"), required=True)


def build_parser() -> argparse.ArgumentParser:
    """Build the command-line interface used by GitHub Actions and tests."""
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    add_detect_parser(subparsers)
    add_manifest_parser(subparsers)
    add_ensure_draft_parser(subparsers)
    return parser


def detect_command(arguments: argparse.Namespace) -> None:
    """Resolve local metadata and preflight remote release state."""
    outputs = detect_versions(
        arguments.repository.resolve(),
        arguments.event,
        arguments.head,
        arguments.before,
    )
    if outputs["changed"] == "true":
        github_token = os.environ.get("GITHUB_TOKEN")
        if not arguments.github_repository or not github_token:
            raise ReleaseMetadataError(
                "changed releases require a GitHub repository and GITHUB_TOKEN"
            )
        action = plan_remote_release(
            arguments.github_repository,
            outputs["tag"],
            outputs["version"],
            validate_sha(arguments.head, "head"),
            github_token,
        )
    else:
        action = "skip"
    outputs["build"] = str(action == "build").lower()
    write_outputs(arguments.github_output, outputs)


def ensure_draft_command(arguments: argparse.Namespace) -> None:
    """Run the fail-closed GitHub draft manager."""
    token = os.environ.get("GH_TOKEN")
    if not token:
        raise ReleaseMetadataError("draft management requires GH_TOKEN")
    if shutil.which("gh") is None:
        raise ReleaseMetadataError("draft management requires the GitHub CLI")
    client = GitHubClient(arguments.repository, token)
    action = ensure_draft_release(
        client,
        arguments.directory.resolve(),
        arguments.manifests.resolve(),
        arguments.tag,
        arguments.version,
        arguments.sha,
        arguments.prerelease == "true",
    )
    print(f"release {arguments.tag}: {action}")


def main() -> int:
    """Dispatch release automation commands with consistent error handling."""
    arguments = build_parser().parse_args()
    try:
        if arguments.command == "detect":
            detect_command(arguments)
        elif arguments.command == "manifest":
            write_build_manifest(
                arguments.directory.resolve(),
                arguments.target,
                arguments.version,
                arguments.output.resolve(),
            )
        else:
            ensure_draft_command(arguments)
    except (OSError, ReleaseMetadataError) as exc:
        print(f"release metadata error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
