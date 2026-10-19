"""I reject incomplete releases, mixed commits and unverified downloads."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from release_platform import architecture, linux_assets, macos_assets
from release_version import update_information

spec = importlib.util.spec_from_file_location("assemble_release", SCRIPTS / "assemble-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseAssetsTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.source = Path(self.directory.name) / "downloaded"
        self.output = Path(self.directory.name) / "dist"
        for system, names in (("linux", linux_assets), ("macos", macos_assets)):
            for arch in ("x86_64", "aarch64"):
                folder = self.source / f"chilpa-{system}-{arch}"
                folder.mkdir(parents=True)
                sums = []
                for name in names("1.2.3", arch):
                    path = folder / name
                    if name.startswith("MANIFEST"):
                        path.write_text(json.dumps(dict(version="1.2.3", source_commit="abc123", platform=f"{system}-{arch}")))
                    else:
                        path.write_bytes(b"release payload")
                    sums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}\n")
                (folder / f"SHA256SUMS-{system}-{arch}").write_text("".join(sums))

    def assemble(self, commit="abc123"):
        release.assemble(self.source, self.output, "1.2.3", commit)

    def test_complete_release_has_all_formats_and_one_checksum_file(self):
        self.assemble()
        self.assertEqual(len(list(self.output.iterdir())), 21)
        for record in (self.output / "SHA256SUMS").read_text().splitlines():
            digest, name = record.split("  ")
            self.assertEqual(hashlib.sha256((self.output / name).read_bytes()).hexdigest(), digest)

    def test_missing_rpm_blocks_the_entire_release(self):
        (self.source / "chilpa-linux-aarch64/chilpa-1.2.3-1.aarch64.rpm").unlink()
        with self.assertRaisesRegex(ValueError, "Missing or unverified"):
            self.assemble()
        self.assertFalse(self.output.exists())

    def test_corrupt_payload_blocks_the_entire_release(self):
        (self.source / "chilpa-macos-x86_64/Chilpa-1.2.3-macos-x86_64.dmg").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "Missing or unverified"):
            self.assemble()
        self.assertFalse(self.output.exists())

    def test_mixed_source_commits_cannot_be_published(self):
        with self.assertRaisesRegex(ValueError, "Mismatched release identity"):
            self.assemble("other")
        self.assertFalse(self.output.exists())

    def test_architecture_aliases_and_delta_channels(self):
        self.assertEqual(architecture("arm64"), "aarch64")
        self.assertEqual(architecture("amd64"), "x86_64")
        for arch in ("x86_64", "aarch64"):
            self.assertTrue(update_information("example/player", "1.2.3", arch).endswith(f"-{arch}.AppImage.zsync"))
            self.assertIn("|v1.2.3-beta|", update_information("example/player", "1.2.3-beta", arch))
        with self.assertRaises(ValueError):
            architecture("armv7")
