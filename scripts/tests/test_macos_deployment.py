"""I check deployment requirements without mistaking the build SDK for a minimum OS."""
import importlib.util
from pathlib import Path
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("package_macos", SCRIPTS / "package-macos.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class DeploymentTests(unittest.TestCase):
    def test_the_sdk_version_does_not_raise_the_deployment_minimum(self):
        commands = "Load command 1\n      cmd LC_BUILD_VERSION\n platform 1\n    minos 14.0\n      sdk 26.4\n"
        self.assertEqual(package.deployment_minimum(commands), "14.0")

    def test_legacy_deployment_commands_keep_their_minimum(self):
        commands = "Load command 1\n      cmd LC_VERSION_MIN_MACOSX\n  version 13.3\n      sdk 14.0\n"
        self.assertEqual(package.deployment_minimum(commands), "13.3")

    def test_multiple_architectures_use_the_strictest_requirement(self):
        commands = "Load command 1\n cmd LC_BUILD_VERSION\n minos 14.0\n sdk 26.4\nLoad command 2\n cmd LC_BUILD_VERSION\n minos 26.0\n sdk 26.4\n"
        self.assertEqual(package.deployment_minimum(commands), "26.0")

    def test_unrelated_source_versions_are_ignored(self):
        commands = "Load command 1\n cmd LC_SOURCE_VERSION\n version 99.1\n"
        self.assertIsNone(package.deployment_minimum(commands))


if __name__ == "__main__":
    unittest.main()
