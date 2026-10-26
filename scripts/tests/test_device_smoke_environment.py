"""I keep native display access separate from disposable application data."""
from pathlib import Path
import os
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
ENVIRONMENT = runpy.run_path(str(ROOT / "scripts/smoke-device-sync.py"), run_name="device_smoke")["environment"]


class DeviceSmokeEnvironment(unittest.TestCase):
    def test_relative_display_keeps_its_compositor_with_a_private_runtime(self):
        with patch.dict(os.environ, {"XDG_RUNTIME_DIR": "/original-runtime", "WAYLAND_DISPLAY": "wayland-fixture"}, clear=True):
            env = ENVIRONMENT(SimpleNamespace(platform="native"), Path("/private-profile"), Path("/fixture.qml"), True)
        self.assertEqual(env["WAYLAND_DISPLAY"], "/original-runtime/wayland-fixture")
        self.assertEqual(env["XDG_RUNTIME_DIR"], "/private-profile/runtime")
        self.assertEqual(env["TMPDIR"], "/private-profile/tmp")
        self.assertEqual(env["XDG_CONFIG_HOME"], "/private-profile/config")
        self.assertEqual(env["XDG_DATA_HOME"], "/private-profile/data")
        self.assertEqual(env["XDG_CACHE_HOME"], "/private-profile/cache")
        self.assertEqual(env["CHILPA_DEVICE_TEST_MOUNTED_ONLY"], "1")

    def test_absolute_display_is_preserved_without_using_the_parent_profile(self):
        with patch.dict(os.environ, {"XDG_RUNTIME_DIR": "/original-runtime", "WAYLAND_DISPLAY": "/display/socket"}, clear=True):
            env = ENVIRONMENT(SimpleNamespace(platform="native"), Path("/private-profile"), Path("/fixture.qml"))
        self.assertEqual(env["WAYLAND_DISPLAY"], "/display/socket")
        self.assertEqual(env["XDG_RUNTIME_DIR"], "/private-profile/runtime")
        self.assertEqual(env["XDG_DATA_HOME"], "/private-profile/data")


if __name__ == "__main__":
    unittest.main()
