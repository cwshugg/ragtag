"""Contract tests for the GitHub Actions CI and release workflows."""

import unittest
from pathlib import Path

WORKFLOWS = Path(__file__).parents[1] / "workflows"
CI_WORKFLOW = WORKFLOWS / "ci.yml"
RELEASE_WORKFLOW = WORKFLOWS / "release.yml"
RELEASE_METADATA = WORKFLOWS / "scripts" / "resolve-release-meta.ps1"
ACTIONLINT_VERSION = "1.7.12"
ACTIONLINT_ARCHIVE_SHA256 = (
    "8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8"
)
ACTIONLINT_MATCHER_SHA256 = (
    "5ec0c56e3947f155d8a9df6361653d97be7969d0bcb10f8c95f6be99b4888f0d"
)
D2_ARCHIVE_SHA256 = "5669ddc46b99e942cc96078f4a4e36d5e62103348f4c05179ede27802fdd87a9"
SETUP_GO_SHA = "b7ad1dad31e06c5925ef5d2fc7ad053ef454303e"


class CiWorkflowTests(unittest.TestCase):
    """Protect the pinned actionlint installation contract."""

    def test_actionlint_avoids_repository_node_modules(self) -> None:
        """The installer must not load action internals from Node modules."""
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("raven-actions/actionlint", workflow)
        self.assertNotIn("actions/github-script", workflow)
        self.assertNotIn("require('@actions/tool-cache')", workflow)
        self.assertNotIn("npm install", workflow)

    def test_actionlint_downloads_and_verifies_pinned_bytes(self) -> None:
        """The archive and matcher must use fixed versions and digests."""
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn(f"ACTIONLINT_VERSION: {ACTIONLINT_VERSION}", workflow)
        self.assertIn(ACTIONLINT_ARCHIVE_SHA256, workflow)
        self.assertIn(ACTIONLINT_MATCHER_SHA256, workflow)
        self.assertIn("--proto '=https' --tlsv1.2 --retry 3", workflow)
        self.assertEqual(workflow.count("sha256sum --check --strict -"), 3)
        self.assertIn("--no-same-owner actionlint", workflow)

    def test_actionlint_path_and_matcher_precede_lint(self) -> None:
        """The verified tool and matcher must be active before linting."""
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        path_update = workflow.index('>> "${GITHUB_PATH}"')
        matcher_update = workflow.index("::add-matcher::")
        lint = workflow.index("run: actionlint -verbose")

        self.assertLess(path_update, lint)
        self.assertLess(matcher_update, lint)

    def test_d2_conformance_is_fully_pinned(self) -> None:
        """D2, Go, helper inputs, and semantic tests are immutable."""
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        helper_module = (
            Path(__file__).parents[2] / "tests" / "tools" / "d2inspect" / "go.mod"
        ).read_text(encoding="utf-8")

        self.assertIn(f"actions/setup-go@{SETUP_GO_SHA}", workflow)
        self.assertIn("go-version: 1.27.0", workflow)
        self.assertIn(D2_ARCHIVE_SHA256, workflow)
        self.assertIn("d2-v${D2_VERSION}-linux-amd64.tar.gz", workflow)
        self.assertIn("sha256sum --check --strict SHA256SUMS", workflow)
        self.assertIn("go mod verify", workflow)
        self.assertIn("go test -mod=readonly ./...", workflow)
        self.assertIn("go build -mod=readonly -trimpath", workflow)
        self.assertIn(
            "cargo test --locked --test d2_conformance -- --ignored", workflow
        )
        self.assertIn("github.com/d2lang/d2 v0.9.0", helper_module)
        self.assertIn("go 1.27.0", helper_module)

    def test_cross_platform_checks_are_compile_only(self) -> None:
        """Windows/macOS cfg coverage remains separate from release assets."""
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("targets: x86_64-pc-windows-msvc, x86_64-apple-darwin", workflow)
        self.assertIn("cargo check --locked --target x86_64-pc-windows-msvc", workflow)
        self.assertIn("cargo check --locked --target x86_64-apple-darwin", workflow)


class ReleaseWorkflowTests(unittest.TestCase):
    """Protect the generic draft-release contract."""

    def test_release_triggers_and_permissions(self) -> None:
        """Cargo changes, tags, and manual runs retain draft permissions."""
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        for required in [
            "branches:\n      - master",
            '- "Cargo.toml"',
            '- "Cargo.lock"',
            'tags:\n      - "v*"',
            "workflow_dispatch:",
            "contents: write",
        ]:
            self.assertIn(required, workflow)
        self.assertNotIn("DISPATCH_TAG", workflow)
        self.assertNotIn("github.event.inputs", workflow)

    def test_draft_creation_and_action_pins(self) -> None:
        """Draft creation and packaging actions use immutable commits."""
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn(
            "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
            workflow,
        )
        self.assertIn(
            "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772",
            workflow,
        )
        self.assertIn(
            "taiki-e/upload-rust-binary-action@"
            "f0d45ae91ee7b8ee928de7a9d04d893a08bcbec6",
            workflow,
        )
        self.assertIn('gh release create "${TAG}"', workflow)
        self.assertIn("--draft", workflow)
        self.assertIn("--generate-notes", workflow)
        self.assertIn('--target "${GITHUB_SHA}"', workflow)
        self.assertNotIn("actions/attest@", workflow)

    def test_metadata_uses_root_package_and_release_idempotency(self) -> None:
        """Metadata resolution derives Cargo version and checks draft releases."""
        script = RELEASE_METADATA.read_text(encoding="utf-8")

        self.assertIn(r"^\[package\]", script)
        self.assertNotIn(r"\[workspace\.package\]", script)
        self.assertIn('$tag = "v$version"', script)
        self.assertNotIn("DISPATCH_TAG", script)
        self.assertIn('gh release view "$tag"', script)
        self.assertIn('"should_release=false"', script)
        self.assertIn('"should_release=true"', script)
        self.assertIn('"tag=$tag"', script)
        self.assertIn("$version -ne $cargoVersion", script)


if __name__ == "__main__":
    unittest.main()
