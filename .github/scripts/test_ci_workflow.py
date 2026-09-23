"""Contract tests for the GitHub Actions quality workflow."""

from pathlib import Path
import unittest


WORKFLOW = Path(__file__).parents[1] / "workflows" / "ci.yml"
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
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("raven-actions/actionlint", workflow)
        self.assertNotIn("actions/github-script", workflow)
        self.assertNotIn("require('@actions/tool-cache')", workflow)
        self.assertNotIn("npm install", workflow)

    def test_actionlint_downloads_and_verifies_pinned_bytes(self) -> None:
        """The archive and matcher must use fixed versions and digests."""
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertIn(f"ACTIONLINT_VERSION: {ACTIONLINT_VERSION}", workflow)
        self.assertIn(ACTIONLINT_ARCHIVE_SHA256, workflow)
        self.assertIn(ACTIONLINT_MATCHER_SHA256, workflow)
        self.assertIn("--proto '=https' --tlsv1.2 --retry 3", workflow)
        self.assertEqual(workflow.count("sha256sum --check --strict -"), 2)
        self.assertIn("--no-same-owner actionlint", workflow)

    def test_actionlint_path_and_matcher_precede_lint(self) -> None:
        """The verified tool and matcher must be active before linting."""
        workflow = WORKFLOW.read_text(encoding="utf-8")

        path_update = workflow.index('>> "${GITHUB_PATH}"')
        matcher_update = workflow.index("::add-matcher::")
        lint = workflow.index("run: actionlint -verbose")

        self.assertLess(path_update, lint)
        self.assertLess(matcher_update, lint)


if __name__ == "__main__":
    unittest.main()
