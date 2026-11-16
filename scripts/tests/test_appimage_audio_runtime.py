import importlib.util
from pathlib import Path
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
