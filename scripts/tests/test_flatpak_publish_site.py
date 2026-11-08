"""What is published comes from the verified mirror only, agrees with what a
Flatpak client reads, and carries no static delta or summary index."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("flatpak_publish_site", SCRIPTS / "flatpak-publish-site.py")
site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(site)

APP = "app/app.chilpa.Chilpa/x86_64/stable"
LOCALE = "runtime/app.chilpa.Chilpa.Locale/x86_64/stable"
A, B, C = "a" * 64, "b" * 64, "c" * 64


def mirror_tree(root):
    """A mirror as ostree leaves it: objects, refs, summary, its own config."""
    for name, data in {
        "objects/ab/" + "1" * 62 + ".commit": b"commit",
        "objects/ab/" + "1" * 62 + ".commitmeta": b"signature",
        "objects/cd/" + "2" * 62 + ".filez": b"file",
        "objects/ef/" + "3" * 62 + ".dirtree": b"tree",
        "refs/heads/" + APP: (A + "\n").encode(),
        "refs/heads/appstream2/x86_64": (C + "\n").encode(),
        "summary": b"summary",
        "summary.sig": b"summary signature",
        "config": b"[remote \"release\"]\nurl=file:///tmp/release\n",
        "release.trustedkeys.gpg": b"key",
        ".lock": b"",
    }.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


class AgreeTests(unittest.TestCase):
    def test_the_client_and_the_mirror_agree_on_every_app_and_runtime_ref(self):
        mirror = {APP: A, LOCALE: B, "appstream2/x86_64": C}
        self.assertEqual(site.agree({APP: A, LOCALE: B}, mirror), 2, "appstream is the mirror's alone")

    def test_an_older_index_beside_a_newer_summary_is_refused(self):
        # The index of release N-1 next to release N's summary: same refs,
        # another commit for the app.
        with self.assertRaises(site.Mismatch) as caught:
            site.agree({APP: B, LOCALE: B}, {APP: A, LOCALE: B})
        self.assertIn(APP, str(caught.exception))
        for client in ({APP: A}, {APP: A, LOCALE: B, "runtime/other/x86_64/stable": C}):
            with self.subTest(client=client), self.assertRaises(site.Mismatch):
                site.agree(client, {APP: A, LOCALE: B})
        with self.assertRaises(site.Mismatch):
            site.agree({}, {"appstream2/x86_64": C})

    def test_a_listing_is_only_refs_and_full_commits(self):
        self.assertEqual(site.listing(f"{APP} {A}\n\n{LOCALE} {B}\n"), {APP: A, LOCALE: B})
        for text in (f"{APP} {A[:12]}", f"{APP}", f"{APP} {A} extra", f"{APP} {A}\n{APP} {B}", f"../x {A}"):
            with self.subTest(text=text), self.assertRaises(site.Mismatch):
                site.listing(text)


class AssembleTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.base = Path(self.folder.name)
        self.mirror = self.base / "mirror"
        mirror_tree(self.mirror)

    def tearDown(self):
        self.folder.cleanup()

    def test_only_the_mirror_s_verified_tree_is_published(self):
        self.assertEqual(site.assemble(self.mirror, self.base / "site"), (4, 2))
        published = sorted(path.relative_to(self.base / "site").as_posix() for path in (self.base / "site").rglob("*") if path.is_file())
        self.assertEqual(published, sorted([
            "config", "summary", "summary.sig", "refs/heads/" + APP, "refs/heads/appstream2/x86_64",
            "objects/ab/" + "1" * 62 + ".commit", "objects/ab/" + "1" * 62 + ".commitmeta",
            "objects/cd/" + "2" * 62 + ".filez", "objects/ef/" + "3" * 62 + ".dirtree"]),
            "not the mirror's config, keyring or lock, and nothing the mirror did not pull")
        self.assertNotIn("remote", (self.base / "site/config").read_text())

    def test_a_static_delta_or_a_summary_index_is_never_published(self):
        for extra in ("deltas/FL/abc/superblock", "delta-indexes/FL/abc.index", "summaries/" + A + ".gz", "summary.idx", "summary.idx.sig"):
            with self.subTest(extra):
                path = self.mirror / extra
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"x")
                with self.assertRaises(site.Mismatch):
                    site.assemble(self.mirror, self.base / "site")
                self.assertFalse((self.base / "site").exists())
                path.unlink()

    def test_anything_else_in_objects_or_a_missing_summary_stops_it(self):
        stray = self.mirror / "objects/ab/index.html"
        stray.write_bytes(b"<script>")
        with self.assertRaises(site.Mismatch):
            site.assemble(self.mirror, self.base / "site")
        stray.unlink()
        (self.mirror / "summary.sig").unlink()
        with self.assertRaises(site.Mismatch):
            site.assemble(self.mirror, self.base / "site")
        self.assertFalse((self.base / "site").exists())


if __name__ == "__main__":
    unittest.main()
