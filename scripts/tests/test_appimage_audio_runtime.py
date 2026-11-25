import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("appimage_audio_runtime", Path(__file__).resolve().parents[1] / "appimage_audio_runtime.py")
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


class PipeWireRuntimeTests(unittest.TestCase):
    def test_deployment_keeps_the_soname_when_the_host_uses_a_versioned_symlink(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-runtime-") as directory:
            directory = Path(directory)
            target = directory / (runtime.SONAME + ".1611.0")
            target.write_bytes(b"fixture")
            soname = directory / runtime.SONAME
            soname.symlink_to(target.name)
            listing = f"{runtime.SONAME} => {soname} (0x1234)\n"
            with patch.object(runtime.subprocess, "check_output", return_value=listing):
                selected = runtime.pipewire_runtime(directory / "executable")
            self.assertEqual(selected, soname)
            self.assertEqual(selected.name, runtime.SONAME)


# A tree shaped like the AppDir: the executable needs both audio clients and
# D-Bus; the PulseAudio client drags its own decoders; D-Bus is shared.
NEEDS = {
    "chilpa-qt": ["libpulse-simple.so.0", "libpulse.so.0", "libpipewire-0.3.so.0", "libdbus-1.so.3", "libQt6Core.so.6"],
    "libpulse-simple.so.0": ["libpulse.so.0"],
    "libpulse.so.0": ["libpulsecommon-16.1.so", "libdbus-1.so.3"],
    "libpulsecommon-16.1.so": ["libsndfile.so.1", "libdbus-1.so.3"],
    "libsndfile.so.1": ["libFLAC.so.12"],
    "libFLAC.so.12": [],
    "libpipewire-0.3.so.0": [],
    "libdbus-1.so.3": ["libsystemd.so.0"],
    "libsystemd.so.0": [],
    "libQt6Core.so.6": [],
}


def tree(root):
    usr = Path(root) / "usr"
    (usr / "bin").mkdir(parents=True)
    (usr / "lib").mkdir()
    (usr / "bin/chilpa-qt").write_bytes(b"\x7fELF executable")
    for name in NEEDS:
        if name != "chilpa-qt":
            (usr / "lib" / name).write_bytes(b"\x7fELF " + name.encode())
    return usr


def needed(path):
    return NEEDS[Path(path).name]


class HostAudioTests(unittest.TestCase):
    def test_each_client_takes_only_what_nothing_else_needs(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            groups = runtime.host_audio_groups(tree(directory), needed)
        self.assertEqual(groups["pipewire"], {"libpipewire-0.3.so.0"})
        self.assertEqual(groups["pulse"], {"libpulse.so.0", "libpulse-simple.so.0", "libpulsecommon-16.1.so",
                                           "libsndfile.so.1", "libFLAC.so.12"})

    def test_a_native_tree_keeps_everything_else(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            usr = tree(directory)
            runtime.strip_host_audio(usr, needed)
            left = sorted(path.name for path in (usr / "lib").iterdir())
        self.assertEqual(left, ["libQt6Core.so.6", "libdbus-1.so.3", "libsystemd.so.0"])

    def launch(self, directory, ldconfig):
        """Builds the image's tree, wraps its launcher and starts it with `ldconfig -p` saying `ldconfig`."""
        appdir = Path(directory) / "AppDir"
        tree(appdir)
        (appdir / "AppRun").write_text('#!/bin/sh\necho "${LD_LIBRARY_PATH:-none}"\n')
        (appdir / "AppRun").chmod(0o755)
        runtime.defer_host_audio(appdir, "x86_64", needed)
        tools = Path(directory) / "tools"
        tools.mkdir()
        (tools / "ldconfig").write_text("#!/bin/sh\nprintf '%s' \"$KNOWN\"\n")
        (tools / "ldconfig").chmod(0o755)
        environment = {"PATH": f"{tools}:/usr/bin:/bin", "KNOWN": ldconfig}
        said = subprocess.run([str(appdir / "AppRun")], env=environment, capture_output=True, text=True, check=True)
        return appdir, said.stdout.strip()

    def test_a_system_with_both_clients_gets_none_of_ours(self):
        known = ("\tlibpipewire-0.3.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpipewire-0.3.so.0\n"
                 "\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n")
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            runtime.check_appdir_audio(appdir)
            self.assertEqual(path, "none")
            self.assertFalse((appdir / "usr/lib/libpipewire-0.3.so.0").exists())
            self.assertTrue((appdir / "usr/lib/libdbus-1.so.3").exists())

    def test_a_system_without_pipewire_gets_ours_and_keeps_its_own_pulse(self):
        known = "\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n"
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            self.assertEqual(path, str((appdir / "usr/lib/host-fallback/pipewire").resolve()))

    def test_another_architecture_s_library_does_not_count(self):
        known = ("\tlibpipewire-0.3.so.0 (libc6) => /lib/i386-linux-gnu/libpipewire-0.3.so.0\n"
                 "\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n")
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            self.assertTrue(path.endswith("host-fallback/pipewire"), path)

    def test_a_system_without_ldconfig_answers_gets_both(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, "")
            self.assertEqual(path.split(":"), [str((appdir / "usr/lib/host-fallback" / group).resolve()) for group in ("pulse", "pipewire")])

    def test_a_native_payload_with_its_own_client_is_refused(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            lib = Path(directory) / "opt/chilpa/lib"
            lib.mkdir(parents=True)
            runtime.check_native_audio(directory)
            (lib / "libpulse.so.0").write_bytes(b"\x7fELF")
            with self.assertRaises(SystemExit):
                runtime.check_native_audio(directory)
