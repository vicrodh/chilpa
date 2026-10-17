"""I keep an absent measurement from passing the memory gate."""
import contextlib
import importlib.machinery
import importlib.util
import io
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader("footprint_gate", str(ROOT / "tools/lint/footprint"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
gate = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(gate)
LINUX_BUS = sys.platform.startswith("linux") and shutil.which("dbus-run-session")


class FootprintTests(unittest.TestCase):
    def test_missing_release_fails_instead_of_skipping(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(gate, "ROOT", Path(directory)), patch.dict(os.environ, {}, clear=True):
                with self.assertRaisesRegex(RuntimeError, "no release binary"):
                    gate.binary_path()

    def test_profile_and_workload_cannot_be_inherited(self):
        original = dict(XDG_CONFIG_HOME="/real/config", XDG_DATA_HOME="/real/data",
                        XDG_CACHE_HOME="/real/cache", XDG_RUNTIME_DIR="/real/runtime",
                        CHILPA_QML_SMOKE="empty.qml", CHILPA_GALLERY="1", CHILPA_SECOND_INSTANCE="1",
                        CHILPA_RENDERER="vulkan", QT_QUICK_BACKEND="rhi", QSG_RHI_BACKEND="vulkan",
                        QML_IMPORT_PATH="/injected", LD_PRELOAD="profiler.so", MALLOC_ARENA_MAX="32",
                        GLIBC_TUNABLES="glibc.malloc.arena_max=32", APPIMAGE="/desktop/other.AppImage",
                        DBUS_STARTER_ADDRESS="desktop-bus", PATH="/usr/bin")
        result = gate.isolated_environment(Path("/private"), original)
        for key in ("CHILPA_QML_SMOKE", "CHILPA_GALLERY", "CHILPA_SECOND_INSTANCE", "QSG_RHI_BACKEND",
                    "QML_IMPORT_PATH", "LD_PRELOAD", "MALLOC_ARENA_MAX", "GLIBC_TUNABLES",
                    "APPIMAGE", "DBUS_STARTER_ADDRESS"):
            self.assertNotIn(key, result)
        for key in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"):
            self.assertTrue(result[key].startswith("/private/"))
            self.assertTrue(original[key].startswith("/real/"))
        self.assertEqual(result["CHILPA_RENDERER"], "software")
        self.assertEqual(result["PATH"], original["PATH"])

    def test_unreadable_zero_or_reused_process_cannot_pass(self):
        for contents in ("Name: fixture\n", "VmRSS: 0 kB\n", "VmRSS: invalid kB\n"):
            with self.subTest(contents=contents), patch.object(gate, "identity", return_value="before"):
                with patch.object(Path, "read_text", return_value=contents):
                    with self.assertRaises(RuntimeError):
                        gate.rss_mib(123, "before")
        with patch.object(gate, "identity", return_value="after"):
            with self.assertRaisesRegex(RuntimeError, "changed identity"):
                gate.rss_mib(123, "before")

    def test_cleanup_never_signals_a_reused_pid(self):
        with patch.object(gate, "identity", return_value="after"), patch.object(gate.os, "kill") as kill:
            gate.stop_owned(Mock(), {123: "before"})
            kill.assert_not_called()

    def test_exceeding_budget_is_a_failure(self):
        result = dict(peak_rss_mib=251, budget_mib=250, main_constructed=True)
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "fixture"
            executable.touch(mode=0o700)
            with patch.object(sys, "argv", ["footprint", "--binary", str(executable)]):
                with patch.object(gate.sys, "platform", "linux"), patch.object(gate.shutil, "which", return_value="dbus-run-session"):
                    with patch.object(gate, "measure", return_value=result), contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(gate.main(), 1)
                        self.assertFalse(result["passed"])

    @unittest.skipUnless(LINUX_BUS, "I require Linux /proc and dbus-run-session")
    def test_live_rss_comes_from_the_owned_process(self):
        self.assertGreater(gate.rss_mib(os.getpid(), gate.identity(os.getpid())), 0)

    @unittest.skipUnless(LINUX_BUS, "I require Linux /proc and dbus-run-session")
    def test_early_exit_and_missing_main_fail_with_real_children(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "fixture"
            executable.write_text("#!" + sys.executable + "\nimport time\ntime.sleep(5)\n")
            executable.chmod(0o700)
            profile = root / "profile"
            profile.mkdir()
            with patch.object(gate, "STARTUP_SECONDS", 0.2), patch.object(gate, "INTERVAL_SECONDS", 0.02):
                with self.assertRaisesRegex(RuntimeError, "did not confirm construction"):
                    gate.measure(executable, profile)
            profile = root / "early"
            profile.mkdir()
            with self.assertRaisesRegex(RuntimeError, "exited before measurement"):
                gate.measure(Path(shutil.which("true")).resolve(), profile)

    @unittest.skipUnless(LINUX_BUS, "I require Linux /proc and dbus-run-session")
    def test_ready_without_an_identified_binary_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "fixture"
            executable.write_text("#!" + sys.executable + "\nimport time\nprint('main window constructed', flush=True)\ntime.sleep(5)\n")
            executable.chmod(0o700)
            profile = root / "profile"
            profile.mkdir()
            with patch.object(gate, "SETTLE_SECONDS", 0.1), patch.object(gate, "INTERVAL_SECONDS", 0.02):
                with self.assertRaisesRegex(RuntimeError, "no application RSS samples"):
                    gate.measure(executable, profile)

    @unittest.skipUnless(LINUX_BUS, "I require Linux /proc and dbus-run-session")
    def test_qml_failure_cannot_pass_by_staying_alive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "fixture"
            executable.write_text("#!" + sys.executable + "\nimport time\nprint('Type Broken unavailable', flush=True)\ntime.sleep(5)\n")
            executable.chmod(0o700)
            profile = root / "profile"
            profile.mkdir()
            with self.assertRaisesRegex(RuntimeError, "type or construction error"):
                gate.measure(executable, profile)


if __name__ == "__main__":
    unittest.main()
