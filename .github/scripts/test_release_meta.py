"""Tests for Cargo-driven draft release detection and verification."""

from __future__ import annotations

import importlib.util
import io
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Any
from unittest import mock

SCRIPT = Path(__file__).with_name("release_meta.py")
WORKFLOW = SCRIPT.parents[1] / "workflows" / "release.yml"
SPEC = importlib.util.spec_from_file_location("release_meta", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
release_meta = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_meta)


class GitRepository:
    """Create isolated repositories for revision-aware metadata tests."""

    def __init__(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.path = Path(self.temporary.name)
        self.run("git", "init", "-q")
        self.run("git", "config", "user.name", "Release Test")
        self.run("git", "config", "user.email", "release@example.invalid")

    def close(self) -> None:
        """Remove the temporary repository."""
        self.temporary.cleanup()

    def run(self, *arguments: str) -> str:
        """Run a checked command in the temporary repository."""
        result = subprocess.run(
            arguments,
            cwd=self.path,
            check=True,
            text=True,
            capture_output=True,
        )
        return result.stdout.strip()

    def write(self, relative_path: str, contents: str) -> None:
        """Write a repository fixture file."""
        destination = self.path / relative_path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(contents, encoding="utf-8")

    def commit(self, message: str) -> str:
        """Commit all fixture files and return the exact commit SHA."""
        self.run("git", "add", ".")
        self.run("git", "commit", "-q", "-m", message)
        return self.run("git", "rev-parse", "HEAD")


class FakeGitHubClient:
    """Model GitHub mutations while recording attestation checks."""

    def __init__(
        self,
        local: Path,
        tag_sha: str | None,
        release: dict[str, Any] | None,
        remote: Path,
        rejected_attestation: str | None = None,
    ) -> None:
        self.local = local
        self.repository = "owner/repository"
        self.current_tag_sha = tag_sha
        self.current_release = release
        self.remote = remote
        self.rejected_attestation = rejected_attestation
        self.operations: list[str] = []
        self.attestations: list[str] = []

    def tag_sha(self, tag: str) -> str | None:
        """Return the modeled tag target."""
        del tag
        return self.current_tag_sha

    def release(self, tag: str) -> dict[str, Any] | None:
        """Return a copy of modeled release metadata."""
        del tag
        if self.current_release is None:
            return None
        return json.loads(json.dumps(self.current_release))

    def create_tag(self, tag: str, sha: str) -> None:
        """Create a tag only when it does not exist."""
        del tag
        if self.current_tag_sha is not None:
            raise AssertionError("test attempted to retarget an existing tag")
        self.current_tag_sha = sha
        self.operations.append("create-tag")

    def _copy_files(self, files: list[Path]) -> None:
        self.remote.mkdir(parents=True, exist_ok=True)
        for source in files:
            shutil.copy2(source, self.remote / source.name)

    def create_release(
        self,
        tag: str,
        title: str,
        prerelease: bool,
        files: list[Path],
    ) -> None:
        """Create modeled draft metadata and copy assets."""
        del tag
        self._copy_files(files)
        self.current_release = {
            "id": 1,
            "name": title,
            "draft": True,
            "prerelease": prerelease,
            "assets": [{"name": path.name} for path in files],
        }
        self.operations.append("create-release")

    def upload(self, tag: str, files: list[Path]) -> None:
        """Replace modeled assets."""
        del tag
        self._copy_files(files)
        assert self.current_release is not None
        self.current_release["assets"] = [{"name": path.name} for path in files]
        self.operations.append("upload")

    def patch_release(
        self,
        release_id: int,
        title: str,
        prerelease: bool,
    ) -> None:
        """Repair modeled draft metadata."""
        del release_id
        assert self.current_release is not None
        self.current_release["name"] = title
        self.current_release["draft"] = True
        self.current_release["prerelease"] = prerelease
        self.operations.append("patch")

    def download(self, tag: str, destination: Path) -> None:
        """Copy modeled remote assets."""
        del tag
        for source in self.remote.iterdir():
            if source.is_file():
                shutil.copy2(source, destination / source.name)
        self.operations.append("download")

    def verify_attestation(self, path: Path, sha: str) -> None:
        """Record exact-commit attestation verification."""
        if path.name == self.rejected_attestation:
            raise release_meta.ReleaseMetadataError(
                f"attestation rejected for {path.name}"
            )
        self.attestations.append(f"{path.name}:{sha}")


def create_release_files(
    directory: Path,
    manifests: Path,
    version: str,
) -> dict[str, str]:
    """Create deterministic package fixtures and build manifests."""
    directory.mkdir()
    manifests.mkdir()
    for platform in release_meta.PLATFORMS:
        archive = release_meta.archive_name(
            version,
            platform["target"],
            platform["extension"],
        )
        (directory / archive).write_bytes(archive.encode())
        digest = release_meta.sha256_file(directory / archive)
        (directory / f"{archive}.sha256").write_text(
            f"{digest}  {archive}\n",
            encoding="utf-8",
        )
        release_meta.write_build_manifest(
            directory,
            platform["target"],
            version,
            manifests / f"{platform['target']}.json",
        )
    return release_meta.load_build_manifests(manifests, version)


class ReleaseMetadataTests(unittest.TestCase):
    """Verify release detection and idempotency decisions."""

    def setUp(self) -> None:
        """Create an empty repository for each metadata test."""
        self.repository = GitRepository()

    def tearDown(self) -> None:
        """Clean up the isolated repository."""
        self.repository.close()

    def test_detects_only_a_resolved_package_version_change(self) -> None:
        """A non-version manifest edit must not schedule a release."""
        self.repository.write(
            "Cargo.toml",
            '[package]\nname = "ragtag"\nversion = "1.0.0"\nedition = "2021"\n',
        )
        self.repository.write("src/main.rs", "fn main() {}\n")
        old = self.repository.commit("initial")
        self.repository.write(
            "Cargo.toml",
            '[package]\nname = "ragtag"\nversion = "1.0.0"\nedition = "2024"\n',
        )
        unchanged = self.repository.commit("metadata only")
        result = release_meta.detect_versions(
            self.repository.path,
            "push",
            unchanged,
            old,
        )
        self.assertEqual(result["old_version"], "1.0.0")
        self.assertEqual(result["version"], "1.0.0")
        self.assertEqual(result["changed"], "false")

        self.repository.write(
            "Cargo.toml",
            '[package]\nname = "ragtag"\nversion = "1.0.5"\nedition = "2024"\n',
        )
        changed = self.repository.commit("release")
        result = release_meta.detect_versions(
            self.repository.path,
            "push",
            changed,
            unchanged,
        )
        self.assertEqual(result["tag"], "v1.0.5")
        self.assertEqual(result["changed"], "true")

    def test_resolves_inherited_workspace_package_version(self) -> None:
        """Cargo metadata must follow `version.workspace = true`."""
        self.repository.write(
            "Cargo.toml",
            '[workspace]\nmembers = ["cli"]\nresolver = "2"\n'
            '[workspace.package]\nversion = "2.1.0-rc.1"\n',
        )
        self.repository.write(
            "cli/Cargo.toml",
            '[package]\nname = "ragtag"\nversion.workspace = true\nedition = "2021"\n',
        )
        self.repository.write("cli/src/main.rs", "fn main() {}\n")
        revision = self.repository.commit("workspace")

        version = release_meta.resolve_revision_version(
            self.repository.path,
            revision,
        )

        self.assertEqual(version, "2.1.0-rc.1")
        self.assertTrue(release_meta.is_prerelease(version))

    def test_initial_push_and_manual_dispatch_use_current_version(self) -> None:
        """Zero bases and manual runs resolve solely from current Cargo data."""
        self.repository.write(
            "Cargo.toml",
            '[package]\nname = "ragtag"\nversion = "3.0.0+build.7"\nedition = "2021"\n',
        )
        self.repository.write("src/main.rs", "fn main() {}\n")
        head = self.repository.commit("initial")

        initial = release_meta.detect_versions(
            self.repository.path,
            "push",
            head,
            release_meta.ZERO_SHA,
        )
        manual = release_meta.detect_versions(
            self.repository.path,
            "workflow_dispatch",
            head,
            None,
        )

        self.assertEqual(initial["changed"], "true")
        self.assertEqual(manual["tag"], "v3.0.0+build.7")
        self.assertEqual(manual["prerelease"], "false")

    def test_missing_nonzero_base_fails_closed(self) -> None:
        """Missing force-push history must never be guessed."""
        self.repository.write(
            "Cargo.toml",
            '[package]\nname = "ragtag"\nversion = "1.0.5"\nedition = "2021"\n',
        )
        self.repository.write("src/main.rs", "fn main() {}\n")
        head = self.repository.commit("initial")

        with self.assertRaisesRegex(
            release_meta.ReleaseMetadataError,
            "force-pushed base",
        ):
            release_meta.detect_versions(
                self.repository.path,
                "push",
                head,
                "1" * 40,
            )

    def test_remote_preflight_always_builds_and_rejects_conflicts(self) -> None:
        """Every attempted release rebuilds before trusted remote validation."""
        sha = "a" * 40
        assets = release_meta.expected_asset_names("1.0.5")
        complete = {
            "draft": True,
            "assets": [{"name": name} for name in assets],
        }

        self.assertEqual(
            release_meta.classify_remote_state(sha, assets, sha, complete),
            "build",
        )
        self.assertEqual(
            release_meta.classify_remote_state(sha, assets, sha, None),
            "build",
        )
        with self.assertRaisesRegex(
            release_meta.ReleaseMetadataError,
            "not release commit",
        ):
            release_meta.classify_remote_state(
                sha,
                assets,
                "b" * 40,
                None,
            )
        with self.assertRaisesRegex(
            release_meta.ReleaseMetadataError,
            "unexpected assets",
        ):
            release_meta.classify_remote_state(
                sha,
                assets,
                sha,
                {"draft": True, "assets": [{"name": "unexpected.txt"}]},
            )

    def test_release_matrix_and_assets_share_one_contract(self) -> None:
        """The matrix produces six archives and six checksum assets."""
        matrix = release_meta.release_matrix()
        assets = release_meta.expected_asset_names("1.0.5")

        self.assertEqual(len(matrix["include"]), 6)
        self.assertEqual(len(assets), 12)
        self.assertIn(
            "ragtag-v1.0.5-x86_64-unknown-linux-gnu.tar.gz",
            assets,
        )
        self.assertIn(
            "ragtag-v1.0.5-aarch64-pc-windows-msvc.zip.sha256",
            assets,
        )

    def test_github_api_only_accepts_configured_missing_statuses(self) -> None:
        """A missing tag's 422 is distinct from an authorization failure."""

        def http_error(status: int) -> release_meta.error.HTTPError:
            return release_meta.error.HTTPError(
                "https://example.invalid",
                status,
                "error",
                {},
                io.BytesIO(b'{"message":"error"}'),
            )

        with mock.patch.object(
            release_meta.request,
            "urlopen",
            side_effect=http_error(422),
        ):
            result = release_meta.github_get(
                "owner/repository",
                "commits/v1.0.5",
                "token",
                missing_statuses=(404, 422),
            )
        self.assertIsNone(result)

        with (
            mock.patch.object(
                release_meta.request,
                "urlopen",
                side_effect=http_error(403),
            ),
            self.assertRaisesRegex(
                release_meta.ReleaseMetadataError,
                "HTTP 403",
            ),
        ):
            release_meta.github_get(
                "owner/repository",
                "commits/v1.0.5",
                "token",
                missing_statuses=(404, 422),
            )


class DraftManagerTests(unittest.TestCase):
    """Verify draft create, repair, no-op, and conflict paths."""

    SHA = "a" * 40
    VERSION = "1.0.5"
    TAG = "v1.0.5"

    def setUp(self) -> None:
        """Create independent local, manifest, and remote directories."""
        self.temporary = tempfile.TemporaryDirectory()
        root = Path(self.temporary.name)
        self.local = root / "dist"
        self.manifests = root / "manifests"
        self.remote = root / "remote"
        self.remote.mkdir()
        self.digests = create_release_files(
            self.local,
            self.manifests,
            self.VERSION,
        )

    def tearDown(self) -> None:
        """Remove draft-manager fixtures."""
        self.temporary.cleanup()

    def release(self, names: set[str], draft: bool = True) -> dict[str, Any]:
        """Create managed release metadata."""
        return {
            "id": 7,
            "name": f"ragtag {self.TAG}",
            "draft": draft,
            "prerelease": False,
            "assets": [{"name": name} for name in sorted(names)],
        }

    def copy_remote(self, names: set[str]) -> None:
        """Copy selected local fixtures to the modeled remote."""
        for name in names:
            shutil.copy2(self.local / name, self.remote / name)

    def ensure_draft(self, client: FakeGitHubClient) -> str:
        """Run the draft manager with standard release identity."""
        return release_meta.ensure_draft_release(
            client,
            self.local,
            self.manifests,
            self.TAG,
            self.VERSION,
            self.SHA,
            False,
        )

    def test_create_makes_immutable_tag_and_verified_draft(self) -> None:
        """A new release creates its tag before the draft."""
        client = FakeGitHubClient(
            self.local,
            None,
            None,
            self.remote,
        )

        action = self.ensure_draft(client)

        self.assertEqual(action, "created")
        self.assertEqual(
            client.operations[:2],
            ["create-tag", "create-release"],
        )
        self.assertEqual(
            set(path.name for path in self.remote.iterdir()), set(self.digests)
        )
        self.assertEqual(len(client.attestations), 24)

    def test_partial_draft_repairs_only_after_remote_verification(self) -> None:
        """A missing platform pair is repaired with retained build outputs."""
        names = set(self.digests)
        archive = next(name for name in names if not name.endswith(".sha256"))
        partial = names - {archive, f"{archive}.sha256"}
        self.copy_remote(partial)
        client = FakeGitHubClient(
            self.local,
            self.SHA,
            self.release(partial),
            self.remote,
        )

        action = self.ensure_draft(client)

        self.assertEqual(action, "repaired")
        self.assertIn("download", client.operations)
        self.assertIn("upload", client.operations)
        self.assertEqual(set(path.name for path in self.remote.iterdir()), names)

    def test_complete_draft_repairs_metadata_after_asset_verification(self) -> None:
        """A complete draft's title is repaired only after byte validation."""
        names = set(self.digests)
        self.copy_remote(names)
        release = self.release(names)
        release["name"] = "incorrect"
        client = FakeGitHubClient(
            self.local,
            self.SHA,
            release,
            self.remote,
        )

        action = self.ensure_draft(client)

        self.assertEqual(action, "repaired")
        self.assertLess(
            client.operations.index("download"),
            client.operations.index("patch"),
        )

    def test_complete_draft_is_verified_without_mutation(self) -> None:
        """A complete draft no-op still validates bytes and attestations."""
        names = set(self.digests)
        self.copy_remote(names)
        client = FakeGitHubClient(
            self.local,
            self.SHA,
            self.release(names),
            self.remote,
        )

        action = self.ensure_draft(client)

        self.assertEqual(action, "verified")
        self.assertNotIn("upload", client.operations)
        self.assertNotIn("patch", client.operations)
        self.assertEqual(client.operations.count("download"), 2)
        self.assertEqual(len(client.attestations), 36)

    def test_published_release_is_verified_without_mutation(self) -> None:
        """A matching published release remains an authenticated no-op."""
        names = set(self.digests)
        self.copy_remote(names)
        client = FakeGitHubClient(
            self.local,
            self.SHA,
            self.release(names, draft=False),
            self.remote,
        )

        action = self.ensure_draft(client)

        self.assertEqual(action, "verified")
        self.assertNotIn("upload", client.operations)
        self.assertNotIn("patch", client.operations)

    def test_mismatched_remote_bytes_fail_closed(self) -> None:
        """Co-located checksums cannot bless substituted remote bytes."""
        names = set(self.digests)
        self.copy_remote(names)
        archive = next(name for name in names if not name.endswith(".sha256"))
        (self.remote / archive).write_bytes(b"substituted")
        digest = release_meta.sha256_file(self.remote / archive)
        (self.remote / f"{archive}.sha256").write_text(
            f"{digest}  {archive}\n",
            encoding="utf-8",
        )
        client = FakeGitHubClient(
            self.local,
            self.SHA,
            self.release(names),
            self.remote,
        )

        with self.assertRaisesRegex(
            release_meta.ReleaseMetadataError,
            "retained build digest",
        ):
            self.ensure_draft(client)
        self.assertNotIn("upload", client.operations)

    def test_invalid_attestation_fails_before_release_mutation(self) -> None:
        """A missing exact-commit attestation blocks release creation."""
        rejected = next(iter(self.digests))
        client = FakeGitHubClient(
            self.local,
            None,
            None,
            self.remote,
            rejected_attestation=rejected,
        )

        with self.assertRaisesRegex(
            release_meta.ReleaseMetadataError,
            "attestation rejected",
        ):
            self.ensure_draft(client)
        self.assertEqual(client.operations, [])

    def test_conflicting_tag_never_mutates_release(self) -> None:
        """A conflicting tag fails before any release mutation."""
        client = FakeGitHubClient(
            self.local,
            "b" * 40,
            None,
            self.remote,
        )

        with self.assertRaisesRegex(
            release_meta.ReleaseMetadataError,
            "existing tag",
        ):
            self.ensure_draft(client)
        self.assertEqual(client.operations, [])


class WorkflowContractTests(unittest.TestCase):
    """Verify that workflow YAML invokes the tested release contract."""

    def test_workflow_uses_generated_matrix_and_separate_artifacts(self) -> None:
        """Build matrix, package artifacts, and digests must remain aligned."""
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("matrix: ${{ steps.release.outputs.matrix }}", workflow)
        self.assertIn(
            "matrix: ${{ fromJSON(needs.detect.outputs.matrix) }}",
            workflow,
        )
        self.assertIn("name: package-${{ matrix.target }}", workflow)
        self.assertIn("name: digest-${{ matrix.target }}", workflow)
        self.assertIn("release_meta.py manifest", workflow)

    def test_workflow_attests_and_uses_tested_draft_manager(self) -> None:
        """The workflow must attest assets before running the draft manager."""
        workflow = WORKFLOW.read_text(encoding="utf-8")

        attest = workflow.index("actions/attest@")
        ensure_draft = workflow.index("release_meta.py ensure-draft")
        self.assertLess(attest, ensure_draft)
        self.assertIn("subject-path: dist/*", workflow)
        self.assertIn("attestations: write", workflow)
        self.assertIn("id-token: write", workflow)
        self.assertIn(
            "actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6",
            workflow,
        )

    def test_workflows_stop_at_verified_drafts(self) -> None:
        """Release automation must stop after securing the draft."""
        workflows = sorted(WORKFLOW.parent.glob("*.yml"))
        contents = "\n".join(path.read_text(encoding="utf-8") for path in workflows)

        self.assertEqual([path.name for path in workflows], ["ci.yml", "release.yml"])
        self.assertIn("release_meta.py ensure-draft", contents)
        self.assertNotIn("draft=false", contents)

    def test_github_commands_pin_tag_and_attestation_identity(self) -> None:
        """GitHub CLI commands enforce an existing tag and exact source SHA."""
        client = release_meta.GitHubClient("owner/repository", "token")
        with mock.patch.object(client, "_gh") as gh:
            client.create_release(
                "v1.0.5",
                "ragtag v1.0.5",
                False,
                [Path("dist/archive")],
            )
            client.verify_attestation(Path("dist/archive"), "a" * 40)

        create = gh.call_args_list[0].args[0]
        verify = gh.call_args_list[1].args[0]
        self.assertIn("--draft", create)
        self.assertIn("--verify-tag", create)
        self.assertNotIn("--target", create)
        self.assertEqual(
            verify,
            [
                "attestation",
                "verify",
                "dist/archive",
                "--repo",
                "owner/repository",
                "--signer-workflow",
                "owner/repository/.github/workflows/release.yml",
                "--source-digest",
                "a" * 40,
                "--source-ref",
                "refs/heads/master",
                "--deny-self-hosted-runners",
            ],
        )


if __name__ == "__main__":
    unittest.main()
