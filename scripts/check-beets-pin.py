#!/usr/bin/env python3
"""Says whether PyPI has a beets newer than the one Chilpa pins.

The pin lives in crates/chilpa-api/data/beets-requirements.txt, with its
hashes, and the bridge is tested against that version only. A newer beets is
a decision for a release: regenerate the requirements with their hashes,
raise BEETS_VERSION in the bridge and the protocol, and run the bridge's
tests. Exit 0 when the pin is the newest, 3 when PyPI has a newer one, 1
when either cannot be read.
"""
import json
import pathlib
import re
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
REQUIREMENTS = ROOT / "crates/chilpa-api/data/beets-requirements.txt"


def numbers(version):
    """The release numbers of a version, without a pre-release suffix."""
    return tuple(int(part) for part in re.match(r"[0-9]+(?:\.[0-9]+)*", version).group(0).split("."))


def main():
    found = re.search(r"^beets==([0-9][0-9.]*)", REQUIREMENTS.read_text(), re.M)
    if not found:
        print(f"no beets pin in {REQUIREMENTS}", file=sys.stderr)
        return 1
    pin = found.group(1)
    request = urllib.request.Request("https://pypi.org/pypi/beets/json", headers={"User-Agent": "chilpa-release-check"})
    try:
        with urllib.request.urlopen(request, timeout=20) as reply:
            latest = json.load(reply)["info"]["version"]
    except (OSError, ValueError, KeyError) as error:
        print(f"PyPI could not be read: {error}", file=sys.stderr)
        return 1
    print(f"beets pinned {pin}, newest on PyPI {latest}")
    return 3 if numbers(latest) > numbers(pin) else 0


if __name__ == "__main__":
    sys.exit(main())
