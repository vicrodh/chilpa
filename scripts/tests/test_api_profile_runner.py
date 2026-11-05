"""The API test launcher owns profiles on every supported runner."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

RUNNER = Path(__file__).resolve().parents[1] / "run-api-tests-isolated.py"
spec = importlib.util.spec_from_file_location("api_profile_runner", RUNNER)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ApiProfileRunnerTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux owns a private session bus")
    def test_linux_child_receives_private_profile_and_bus_until_it_exits(self):
        child = '''
import json,os
from pathlib import Path
root=Path(os.environ["CHILPA_TEST_PROFILE_ROOT"])
assert (root/".chilpa-api-test-profile").read_bytes()==b"chilpa-api-unit-tests-v1\\n"
for key,folder in [("XDG_CONFIG_HOME","config"),("XDG_DATA_HOME","data"),("XDG_CACHE_HOME","cache")]:
    assert Path(os.environ[key])==root/folder
    assert (root/folder).is_dir()
assert os.environ["RUST_TEST_THREADS"]=="1"
assert os.environ["DBUS_SESSION_BUS_ADDRESS"]!="unix:path=/invalid-parent-bus"
(root/"data"/"child-result").write_text("private test data")
print(json.dumps({"root":str(root)}))
raise SystemExit(17)
'''
        environment = dict(os.environ, DBUS_SESSION_BUS_ADDRESS="unix:path=/invalid-parent-bus")
        result = subprocess.run([sys.executable, str(RUNNER), "--", sys.executable, "-c", child],
                                env=environment, capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 17, result.stderr)
        root = Path(json.loads(result.stdout)["root"])
        self.assertFalse(root.exists(), "the profile must be removed after the child exits")

    def test_native_runners_preserve_arguments_without_requiring_dbus(self):
        for platform in ("darwin", "win32"):
            with self.subTest(platform=platform):
                roots = []

                def child(command, *, env, check):
                    self.assertEqual(command, ["test command", "an argument with spaces"])
                    self.assertFalse(check)
                    root = Path(env["CHILPA_TEST_PROFILE_ROOT"])
                    self.assertTrue((root / ".chilpa-api-test-profile").is_file())
                    self.assertTrue((root / "data").is_dir())
                    self.assertEqual(env["RUST_TEST_THREADS"], "1")
                    roots.append(root)
                    return subprocess.CompletedProcess(command, 23)

                with patch.object(sys, "platform", platform), \
                        patch.object(sys, "argv", [str(RUNNER), "--", "test command", "an argument with spaces"]), \
                        patch.object(runner.subprocess, "run", side_effect=child):
                    self.assertEqual(runner.main(), 23)
                self.assertTrue(roots)
                self.assertTrue(all(not root.exists() for root in roots))


if __name__ == "__main__":
    unittest.main()
