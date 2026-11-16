"""I check prerelease identity without building or publishing a release."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from release_version import not_older, release_version, update_information


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

    def test_a_release_replaces_only_an_older_or_equal_one(self):
        for version, published in [("0.1.3", "0.1.2"), ("0.1.2", "0.1.2"), ("0.2.0", "0.1.10"), ("0.1.10", "0.1.9"),
                                   ("1.0.0", "1.0.0-rc.1"), ("1.0.0-rc.2", "1.0.0-rc.1"), ("1.0.0-rc.10", "1.0.0-rc.9"),
                                   ("1.0.0-beta", "1.0.0-alpha"), ("1.0.0-alpha.1", "1.0.0-alpha")]:
            with self.subTest(version=version, published=published):
                self.assertTrue(not_older(version, published))
                self.assertEqual(not_older(published, version), version == published)
        self.assertFalse(not_older("1.0.0-rc.1", "1.0.0"), "a prerelease comes before its release")
        self.assertFalse(not_older("1.0.0-1", "1.0.0-alpha"), "numeric identifiers come before others")

    def test_ci_refuses_to_publish_over_a_newer_release(self):
        command = [sys.executable, str(SCRIPTS / "release_version.py"), "--version", "0.1.2"]
        older = subprocess.run(command + ["--not-older-than", "0.1.3"], capture_output=True, text=True)
        self.assertNotEqual(older.returncode, 0)
        self.assertIn("0.1.2 is older than 0.1.3", older.stderr)
        for published in ["0.1.2", "0.1.1"]:
            self.assertEqual(subprocess.run(command + ["--not-older-than", published], capture_output=True).returncode, 0)
        garbage = subprocess.run(command + ["--not-older-than", "latest"], capture_output=True, text=True)
        self.assertNotEqual(garbage.returncode, 0, "an unreadable published version refuses, it does not pass")


if __name__ == "__main__":
    unittest.main()
