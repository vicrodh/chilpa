"""The linter's screenshot waiver applies to a dispatch only, and only for screenshots this commit holds."""
import json
from pathlib import Path
import sys
import tempfile
import tomllib
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from flatpak_lint_exceptions import EXCEPTIONS, METAINFO, RULE, lint_exceptions

BASE = {"app.chilpa.Chilpa": {"appstream-external-screenshot-url": "mirroring is Flathub's step"}}


def checkout(directory, images, files=()):
    # One level down, so a path can point at a real file outside the checkout.
    root = Path(directory) / "checkout"
    (Path(directory) / "outside.png").write_bytes(b"png")
    (root / EXCEPTIONS).parent.mkdir(parents=True)
    (root / EXCEPTIONS).write_text(json.dumps(BASE))
    (root / METAINFO).parent.mkdir(parents=True)
    shots = "".join(f"<screenshot><image>{image}</image></screenshot>" for image in images)
    (root / METAINFO).write_text(f"<component><screenshots>{shots}</screenshots></component>")
    for name in files:
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_bytes(b"png")
    return root


PINNED = "https://raw.githubusercontent.com/vicrodh/chilpa/v1.2.3/assets/captures/one.png"


class LintExceptionsTests(unittest.TestCase):
    def test_a_tag_run_keeps_the_screenshot_rule(self):
        with tempfile.TemporaryDirectory() as directory:
            root = checkout(directory, [PINNED], ["assets/captures/one.png"])
            self.assertEqual(lint_exceptions(root, "1.2.3", tagged=True), BASE)

    def test_a_dispatch_waives_it_when_every_screenshot_is_in_the_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = checkout(directory, [PINNED], ["assets/captures/one.png"])
            rules = lint_exceptions(root, "1.2.3", tagged=False)["app.chilpa.Chilpa"]
            self.assertIn(RULE, rules)
            self.assertIn("appstream-external-screenshot-url", rules)

    def test_a_dispatch_refuses_a_missing_file_another_pin_or_a_path_out_of_the_checkout(self):
        escaping = "https://raw.githubusercontent.com/vicrodh/chilpa/v1.2.3/../outside.png"
        cases = [
            ([PINNED], []),
            (["https://raw.githubusercontent.com/vicrodh/chilpa/main/assets/captures/one.png"], ["assets/captures/one.png"]),
            ([escaping], []),
            ([], []),
        ]
        for images, files in cases:
            with self.subTest(images=images), tempfile.TemporaryDirectory() as directory:
                root = checkout(directory, images, files)
                with self.assertRaises(ValueError):
                    lint_exceptions(root, "1.2.3", tagged=False)

    def test_the_real_metainfo_pins_files_of_this_checkout_to_the_workspace_version(self):
        root = SCRIPTS.parent
        version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        rules = lint_exceptions(root, version, tagged=False)
        self.assertIn(RULE, rules["app.chilpa.Chilpa"])


if __name__ == "__main__":
    unittest.main()
