"""The Flathub linter's exceptions for one run of release-flatpak.yml.

The metainfo pins its screenshots to the release tag. Before the tag exists,
in a manual dispatch, those URLs answer 404, appstream compose drops them and
the linter reports appstream-missing-screenshots. A dispatch checks instead
that each URL names a file of the commit it builds, at the tag's path, and
waives that one rule. A tag run keeps it.
"""
import argparse
import json
from pathlib import Path
import re

APP_ID = "app.chilpa.Chilpa"
RULE = "appstream-missing-screenshots"
EXCEPTIONS = Path("packaging/flatpak/lint-exceptions.json")
METAINFO = Path("packaging/linux/app.chilpa.Chilpa.metainfo.xml")
_IMAGE = re.compile(r"<image>\s*([^<\s]+)\s*</image>")


def screenshot_paths(metainfo, version):
    """The checkout path each screenshot URL names; a URL not pinned to the tag is refused."""
    prefix = f"https://raw.githubusercontent.com/vicrodh/chilpa/v{version}/"
    urls = _IMAGE.findall(metainfo)
    if not urls:
        raise ValueError("The metainfo lists no screenshots.")
    paths = []
    for url in urls:
        if not url.startswith(prefix):
            raise ValueError(f"{url} is not pinned to v{version}.")
        paths.append(url[len(prefix):])
    return paths


def lint_exceptions(root, version, tagged):
    """The exceptions file's rules, plus the screenshot waiver for a dispatch."""
    root = Path(root).resolve()
    rules = json.loads((root / EXCEPTIONS).read_text())
    if tagged:
        return rules
    paths = screenshot_paths((root / METAINFO).read_text(), version)
    for path in paths:
        target = (root / path).resolve()
        if not target.is_relative_to(root) or not target.is_file():
            raise ValueError(f"The screenshot {path} is not a file of this commit.")
    rules[APP_ID][RULE] = (
        f"A dispatch builds before v{version} exists, so the screenshot URLs pinned to it answer 404; "
        f"each of the {len(paths)} names a file of this commit."
    )
    return rules


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--version", required=True)
    parser.add_argument("--event", required=True, help="GITHUB_EVENT_NAME; only push is a tag run")
    parser.add_argument("--output", required=True)
    parser.add_argument("--root", default=Path(__file__).resolve().parents[1])
    arguments = parser.parse_args()
    tagged = arguments.event == "push"
    rules = lint_exceptions(arguments.root, arguments.version, tagged)
    Path(arguments.output).write_text(json.dumps(rules, indent=4) + "\n")
    if not tagged:
        print(f"flatpak-lint-exceptions: {RULE} waived until v{arguments.version} exists; every screenshot names a file of this commit")


if __name__ == "__main__":
    main()
