"""I exercise the installer with isolated homes and fixture downloads."""
import hashlib
import json
import os
import platform
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.user = self.root / "user with spaces"
        self.user.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.env = dict(os.environ, HOME=str(self.user), PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        TEST_SYSTEM="Linux", TEST_ARCH="x86_64", TEST_RUNNING="0", TEST_ROOT=str(self.root))
        self.stub("id", "#!/bin/sh\nprintf '1000\\n'\n")
        self.stub("uname", '#!/bin/sh\nif [ "$1" = -s ]; then echo "$TEST_SYSTEM"; else echo "$TEST_ARCH"; fi\n')
        self.stub("getconf", "#!/bin/sh\necho 'glibc 2.39'\n")
        self.stub("pgrep", '#!/bin/sh\n[ "$TEST_RUNNING" = 1 ]\n')
        self.stub("curl", f'''#!{sys.executable}
import os, shutil, sys
from pathlib import Path
args=sys.argv[1:]
source=Path(os.environ["TEST_ROOT"])/("release.json" if "/repos/" in args[-1] else "package")
shutil.copyfile(source, args[args.index("--output")+1])
''')
        self.metadata()

    def stub(self, name, contents):
        path = self.bin / name
        path.write_text(contents)
        path.chmod(0o755)

    def metadata(self, version="1.2.3", arch="x86_64", system="linux"):
        payload = bytearray(64)
        payload[:4] = b"\x7fELF"
        payload[8:11] = b"AI\x02"
        payload[18:20] = b"\x3e\x00" if arch == "x86_64" else b"\xb7\x00"
        payload.extend(version.encode())
        (self.root / "package").write_bytes(payload)
        name = f"Chilpa-{version}-{arch}.AppImage" if system == "linux" else f"Chilpa-{version}-macos-{arch}.zip"
        self.release = dict(tag_name=f"v{version}", draft=False, prerelease="-" in version, assets=[dict(name=name,
            browser_download_url=f"https://github.com/vicrodh/chilpa/releases/download/v{version}/{name}",
            digest="sha256:" + hashlib.sha256(payload).hexdigest(), size=len(payload))])
        self.write_metadata()

    def write_metadata(self):
        (self.root / "release.json").write_text(json.dumps(self.release))

    def run_installer(self, *args):
        return subprocess.run(["bash", str(INSTALLER), *args], env=self.env, capture_output=True, text=True, timeout=20)

    @property
    def destination(self):
        return self.user / ".local/opt/chilpa/Chilpa.AppImage"

    def test_install_and_update_owned_image_on_both_linux_architectures(self):
        for arch in ("x86_64", "aarch64"):
            with self.subTest(arch=arch):
                self.user = self.root / f"user with spaces {arch}"
                self.user.mkdir()
                self.env["HOME"] = str(self.user)
                self.env["TEST_ARCH"] = arch
                self.metadata("1.2.3", arch)
                result = self.run_installer()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.destination.read_bytes(), (self.root / "package").read_bytes())
                self.metadata("1.2.4", arch)
                result = self.run_installer()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.destination.read_bytes(), (self.root / "package").read_bytes())
                self.assertFalse((self.destination.parent / ".chilpa-install.lock").exists())

    def test_check_selects_both_macos_architectures_without_installing(self):
        self.env["TEST_SYSTEM"] = "Darwin"
        for arch in ("x86_64", "arm64"):
            self.env["TEST_ARCH"] = arch
            canonical = "aarch64" if arch == "arm64" else arch
            if sys.platform == "darwin" and platform.machine() == "arm64":
                canonical = "aarch64"
            self.metadata(arch=canonical, system="macos")
            result = self.run_installer("--check")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(f"macos-{canonical}.zip", result.stdout)
        self.assertFalse((self.user / "Applications").exists())

    def test_corrupt_download_preserves_the_existing_installation(self):
        self.assertEqual(self.run_installer().returncode, 0)
        before = self.destination.read_bytes()
        self.metadata("1.2.4")
        (self.root / "package").write_bytes(b"corrupt")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum mismatch", result.stderr)
        self.assertEqual(self.destination.read_bytes(), before)

    def test_unowned_installation_is_preserved(self):
        self.destination.parent.mkdir(parents=True)
        self.destination.write_bytes(b"my installation")
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.assertEqual(self.destination.read_bytes(), b"my installation")

    def test_wrong_architecture_and_missing_digest_are_rejected(self):
        self.metadata(arch="aarch64")
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.metadata()
        del self.release["assets"][0]["digest"]
        self.write_metadata()
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_betas_require_an_explicit_version_and_running_apps_are_preserved(self):
        self.metadata("1.2.3-beta.2")
        self.assertNotEqual(self.run_installer().returncode, 0)
        result = self.run_installer("--version=1.2.3-beta.2")
        self.assertEqual(result.returncode, 0, result.stderr)
        before = self.destination.read_bytes()
        self.metadata("1.2.4")
        self.env["TEST_RUNNING"] = "1"
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.assertEqual(self.destination.read_bytes(), before)

    def test_default_updates_never_downgrade_a_newer_installation(self):
        self.metadata("2.0.0-beta.1")
        self.assertEqual(self.run_installer("--version=2.0.0-beta.1").returncode, 0)
        before = self.destination.read_bytes()
        self.metadata("1.2.4")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("installed version is newer", result.stderr)
        self.assertEqual(self.destination.read_bytes(), before)
        self.assertEqual(self.run_installer("--version=1.2.4").returncode, 0)
