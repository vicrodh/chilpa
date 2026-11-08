"""The repository archive is unpacked as data: only an OSTree repository's
own files, never a link, a git folder or a path out of repo/."""
import io
from pathlib import Path
import stat
import sys
import tarfile
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import importlib.util

spec = importlib.util.spec_from_file_location("flatpak_repo_archive", SCRIPTS / "flatpak-repo-archive.py")
archive_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive_module)
unpack, Refused = archive_module.unpack, archive_module.Refused

OBJECT = "repo/objects/ab/" + "c" * 62 + ".filez"
GOOD = {
    "repo/config": b"[core]\nmode=archive-z2\n",
    "repo/summary": b"summary",
    "repo/summary.sig": b"sig",
    "repo/summaries/" + "d" * 64 + ".gz": b"gz",
    OBJECT: b"object",
    "repo/objects/12/" + "e" * 62 + ".commitmeta2": b"meta",
    "repo/refs/heads/app/app.chilpa.Chilpa/x86_64/stable": b"f" * 64,
    "repo/deltas/FL/2fzxFR3YlE58oNurZiPnvmAg5IZAtf7x5Pf8EpVdI/superblock": b"delta",
    "repo/delta-indexes/FL/2fzxFR3YlE58oNurZiPnvmAg5IZAtf7x5Pf8EpVdI.index": b"index",
    "repo/.lock": b"",
    "repo/tmp/cache/flatpak-delta-cache": b"cache",
}


def archive(path, files, extra=()):
    with tarfile.open(path, "w") as tar:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o755
            tar.addfile(info, io.BytesIO(data))
        for info in extra:
            tar.addfile(info, io.BytesIO(b"x" * info.size) if info.isreg() else None)


class RepoArchiveTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.base = Path(self.folder.name)

    def tearDown(self):
        self.folder.cleanup()

    def test_a_repository_s_own_files_are_unpacked_without_execute_bits(self):
        archive(self.base / "repo.tar", GOOD)
        self.assertEqual(unpack(self.base / "repo.tar", self.base / "out"), len(GOOD) - 2, "the lock and tmp are left out")
        written = self.base / "out" / OBJECT
        self.assertEqual(written.read_bytes(), b"object")
        self.assertFalse(written.stat().st_mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH))
        self.assertFalse((self.base / "out/repo/tmp").exists() or (self.base / "out/repo/.lock").exists())

    def test_anything_else_stops_the_unpacking_before_a_file_is_written(self):
        cases = {
            "a git hook": dict(GOOD, **{"repo/.git/hooks/pre-commit": b"#!/bin/sh\n"}),
            "a git folder by another case": dict(GOOD, **{"repo/.GIT/config": b"x"}),
            "a page beside the repository": dict(GOOD, **{"repo/index.html": b"<script>"}),
            "an object of another kind": dict(GOOD, **{"repo/objects/ab/" + "c" * 62 + ".html": b"x"}),
            "a file outside repo/": dict(GOOD, **{"site/CNAME": b"x"}),
            "a path out of the folder": dict(GOOD, **{"repo/../escape": b"x"}),
            "an absolute path": dict(GOOD, **{"/tmp/escape": b"x"}),
            "no config": {name: data for name, data in GOOD.items() if name != "repo/config"},
        }
        for case, files in cases.items():
            with self.subTest(case):
                target = self.base / case.replace(" ", "-")
                archive(self.base / "bad.tar", files)
                with self.assertRaises(Refused):
                    unpack(self.base / "bad.tar", target)
                self.assertFalse(target.exists(), "nothing was written")

    def test_links_devices_and_a_second_copy_are_refused(self):
        link = tarfile.TarInfo("repo/refs/heads/link")
        link.type, link.linkname = tarfile.SYMTYPE, "/etc/passwd"
        hard = tarfile.TarInfo("repo/refs/heads/hard")
        hard.type, hard.linkname = tarfile.LNKTYPE, "repo/config"
        fifo = tarfile.TarInfo("repo/refs/heads/fifo")
        fifo.type = tarfile.FIFOTYPE
        for member in (link, hard, fifo):
            with self.subTest(member.name):
                archive(self.base / "links.tar", GOOD, [member])
                with self.assertRaises(Refused):
                    unpack(self.base / "links.tar", self.base / "links")
        twice = self.base / "twice.tar"
        archive(twice, GOOD, [tarfile.TarInfo("repo/config")])
        with self.assertRaises(Refused):
            unpack(twice, self.base / "twice")
        self.assertFalse((self.base / "twice").exists())

    def test_an_existing_destination_is_never_written_into(self):
        archive(self.base / "repo.tar", GOOD)
        (self.base / "out").mkdir()
        with self.assertRaises(Refused):
            unpack(self.base / "repo.tar", self.base / "out")


if __name__ == "__main__":
    unittest.main()
