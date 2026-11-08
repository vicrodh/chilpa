"""The descriptors of the Flatpak repository, checked without building it."""
import base64
import configparser
import importlib.util
from pathlib import Path
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("flatpak_descriptors", SCRIPTS / "flatpak-descriptors.py")
descriptors = importlib.util.module_from_spec(spec)
spec.loader.exec_module(descriptors)

KEY = b"\x99\x01\x0d\x04binary-public-key"


def parsed(text, section):
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
    parser.read_string(text)
    return dict(parser[section])


class FlatpakDescriptorTests(unittest.TestCase):
    def test_the_repo_and_the_ref_name_one_repository_and_key(self):
        files = descriptors.descriptors("https://flatpak.example.org/repo", KEY, "stable")
        repo = parsed(files["chilpa.flatpakrepo"], "Flatpak Repo")
        ref = parsed(files["app.chilpa.Chilpa.flatpakref"], "Flatpak Ref")
        self.assertEqual(repo["Url"], "https://flatpak.example.org/repo/")
        self.assertEqual(ref["Url"], repo["Url"])
        self.assertEqual(base64.b64decode(repo["GPGKey"]), KEY)
        self.assertEqual(ref["GPGKey"], repo["GPGKey"])
        self.assertEqual((ref["Name"], ref["Branch"], ref["IsRuntime"]), ("app.chilpa.Chilpa", "stable", "false"))
        self.assertEqual(ref["RuntimeRepo"], "https://dl.flathub.org/repo/flathub.flatpakrepo")
        self.assertNotIn("Icon", ref)

    def test_an_address_that_is_not_plain_https_is_refused(self):
        for url in ["http://flatpak.example.org/repo/", "https:///repo/", "https://flatpak.example.org/repo/?x=1",
                    "https://flatpak.example.org/re po/", "file:///srv/repo/"]:
            with self.subTest(url=url), self.assertRaises(ValueError):
                descriptors.descriptors(url, KEY, "stable")

    def test_an_armored_or_empty_key_and_a_bad_branch_are_refused(self):
        for key, branch in [(b"", "stable"), (b"-----BEGIN PGP PUBLIC KEY BLOCK-----\n", "stable"),
                            (KEY, ""), (KEY, "stable/beta"), (KEY, "stable\nUrl=https://evil.example/")]:
            with self.subTest(key=key, branch=branch), self.assertRaises(ValueError):
                descriptors.descriptors("https://flatpak.example.org/repo/", key, branch)


if __name__ == "__main__":
    unittest.main()
