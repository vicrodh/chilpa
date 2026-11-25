import importlib.util
import os
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


def elf(bits=64, machine=0x3E):
    """The first bytes of a shared library's ELF header."""
    header = bytearray(64)
    header[0:4] = b"\x7fELF"
    header[4] = 2 if bits == 64 else 1
    header[5] = header[6] = 1
    header[16:18] = (3).to_bytes(2, "little")
    header[18:20] = machine.to_bytes(2, "little")
    return bytes(header)


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

    def launch(self, directory, ldconfig, library_path=None):
        """Builds the image's tree, wraps its launcher and starts it with `ldconfig -p` saying `ldconfig`,
        searching no system folder, so the machine running the test does not answer for the one tested."""
        appdir = Path(directory) / "AppDir"
        tree(appdir)
        (appdir / "AppRun").write_text('#!/bin/sh\necho "${LD_LIBRARY_PATH:-none}"\n')
        (appdir / "AppRun").chmod(0o755)
        runtime.defer_host_audio(appdir, "x86_64", needed, dirs=())
        tools = Path(directory) / "tools"
        tools.mkdir()
        (tools / "ldconfig").write_text("#!/bin/sh\nprintf '%s' \"$KNOWN\"\n")
        (tools / "ldconfig").chmod(0o755)
        environment = {"PATH": f"{tools}:/usr/bin:/bin", "KNOWN": ldconfig}
        if library_path is not None:
            environment["LD_LIBRARY_PATH"] = library_path
        said = subprocess.run([str(appdir / "AppRun")], env=environment, capture_output=True, text=True, check=True)
        return appdir, said.stdout.strip()

    def test_a_system_with_both_clients_gets_none_of_ours(self):
        known = ("\tlibpipewire-0.3.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpipewire-0.3.so.0\n"
                 "\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n"
                 "\tlibpulse-simple.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse-simple.so.0\n")
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            runtime.check_appdir_audio(appdir)
            self.assertEqual(path, "none")
            self.assertFalse((appdir / "usr/lib/libpipewire-0.3.so.0").exists())
            self.assertTrue((appdir / "usr/lib/libdbus-1.so.3").exists())

    def test_a_system_without_pipewire_gets_ours_and_keeps_its_own_pulse(self):
        known = ("\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n"
                 "\tlibpulse-simple.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse-simple.so.0\n")
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            self.assertEqual(path, str((appdir / "usr/lib/host-fallback/pipewire").resolve()))

    def test_another_architecture_s_library_does_not_count(self):
        known = ("\tlibpipewire-0.3.so.0 (libc6) => /lib/i386-linux-gnu/libpipewire-0.3.so.0\n"
                 "\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n"
                 "\tlibpulse-simple.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse-simple.so.0\n")
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            self.assertTrue(path.endswith("host-fallback/pipewire"), path)

    def test_a_system_with_libpulse_but_not_its_simple_client_gets_ours(self):
        known = ("\tlibpipewire-0.3.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpipewire-0.3.so.0\n"
                 "\tlibpulse.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libpulse.so.0\n")
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir, path = self.launch(directory, known)
            self.assertEqual(path, str((appdir / "usr/lib/host-fallback/pulse").resolve()))

    def test_libraries_the_loader_would_find_count_when_ldconfig_says_nothing(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            system = Path(directory) / "system-lib"
            system.mkdir()
            for name in ("libpipewire-0.3.so.0", "libpulse.so.0", "libpulse-simple.so.0"):
                (system / name).write_bytes(elf())
            appdir, path = self.launch(directory, "", library_path=str(system))
            self.assertEqual(path, str(system), "nothing of ours goes before what the system has")

    def test_a_folder_with_spaces_is_searched_as_one_folder(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            system = Path(directory) / "Audio Runtime" / "lib"
            system.mkdir(parents=True)
            for name in ("libpipewire-0.3.so.0", "libpulse.so.0", "libpulse-simple.so.0"):
                (system / name).write_bytes(elf())
            appdir, path = self.launch(directory, "", library_path=str(system))
            self.assertEqual(path, str(system))

    def test_another_architecture_s_library_in_a_folder_does_not_count(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            system = Path(directory) / "system-lib"
            system.mkdir()
            (system / "libpipewire-0.3.so.0").write_bytes(elf())
            (system / "libpulse.so.0").write_bytes(elf(bits=32, machine=3))
            appdir, path = self.launch(directory, "", library_path=str(system))
            self.assertEqual(path.split(":"), [str((appdir / "usr/lib/host-fallback/pulse").resolve()), str(system)])

    def test_a_system_where_nothing_finds_the_libraries_gets_both(self):
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


class HostAudioShapeTests(unittest.TestCase):
    def test_a_client_named_by_a_link_moves_with_its_file(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir = Path(directory) / "AppDir"
            usr = tree(appdir)
            (usr / "lib/libpulse.so.0").rename(usr / "lib/libpulse.so.0.24.3")
            (usr / "lib/libpulse.so.0").symlink_to("libpulse.so.0.24.3")
            (appdir / "AppRun").write_text("#!/bin/sh\n")
            alias = {"libpulse.so.0.24.3": NEEDS["libpulse.so.0"]}
            groups = runtime.defer_host_audio(appdir, "x86_64", lambda path: alias.get(Path(path).name) or NEEDS[Path(path).name], dirs=())
            self.assertIn("libpulse.so.0.24.3", groups["pulse"])
            moved = appdir / "usr/lib/host-fallback/pulse"
            self.assertTrue((moved / "libpulse.so.0").is_symlink())
            self.assertTrue((moved / "libpulse.so.0").resolve().is_file())
            self.assertFalse(os.path.lexists(appdir / "usr/lib/libpulse.so.0"))
            (appdir / "usr/lib/host-fallback/pipewire/libpipewire-0.3.so.0").write_bytes(b"\x7fELF")
            runtime.check_appdir_audio(appdir)

    def test_an_image_that_keeps_a_pulse_client_ahead_is_refused(self):
        with tempfile.TemporaryDirectory(prefix="chilpa-audio-") as directory:
            appdir = Path(directory) / "AppDir"
            tree(appdir)
            (appdir / "AppRun").write_text("#!/bin/sh\n")
            runtime.defer_host_audio(appdir, "x86_64", needed, dirs=())
            (appdir / "usr/lib/libpulse-simple.so.0").write_bytes(b"\x7fELF")
            with self.assertRaises(SystemExit):
                runtime.check_appdir_audio(appdir)
