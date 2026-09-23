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
        self.assertEqual(workflow.count("sha256sum --check --strict -"), 2)
        self.assertIn("--no-same-owner actionlint", workflow)

    def test_actionlint_path_and_matcher_precede_lint(self) -> None:
        """The verified tool and matcher must be active before linting."""
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        path_update = workflow.index('>> "${GITHUB_PATH}"')
        matcher_update = workflow.index("::add-matcher::")
        lint = workflow.index("run: actionlint -verbose")

        self.assertLess(path_update, lint)
        self.assertLess(matcher_update, lint)


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
