"""I check prerelease identity without building or publishing a release."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from release_version import release_version, update_information


class ReleaseVersionTests(unittest.TestCase):
    def test_stable_and_prerelease_versions_keep_their_identity(self):
        for version, prerelease in [("0.1.0", False), ("0.1.0-beta", True), ("0.1.0-beta.2", True), ("1.2.3-rc.1", True)]:
            with self.subTest(version=version):
                self.assertEqual(release_version(version), {"version": version, "tag": "v" + version, "prerelease": prerelease})

    def test_invalid_versions_cannot_become_tags_or_environment_records(self):
        for version in ["v0.1.0", "01.1.0", "0.1.0-beta.01", "0.1.0-", "0.1.0+local", "0.1.0\nINJECT=1", "0.1.0/other"]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                release_version(version)

    def test_beta_fallback_is_pinned_and_stable_updates_remain_stable(self):
        self.assertIn("|v0.1.0-beta|", update_information("example/player", "0.1.0-beta"))
        self.assertIn("|latest|", update_information("example/player", "0.1.0"))
        with self.assertRaises(ValueError):
            update_information("example/player/other", "0.1.0-beta")

    def test_ci_records_prerelease_and_rejects_a_different_tag(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            environment = dict(os.environ, RELEASE_EVENT="push", RELEASE_REF="refs/tags/v0.1.0-beta")
            command = [sys.executable, str(SCRIPTS / "release_version.py"), "--version", "0.1.0-beta", "--github-output", str(output)]
            result = subprocess.run(command, env=environment, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output.read_text(), "version=0.1.0-beta\nprerelease=true\n")
            environment["RELEASE_REF"] = "refs/tags/v0.1.0"
            result = subprocess.run(command, env=environment, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("tag and workspace version differ", result.stderr)


if __name__ == "__main__":
    unittest.main()
