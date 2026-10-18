#!/usr/bin/env python3
"""Download the pinned hunspell dictionaries the language lint reads, and verify them.

A distribution's dictionaries differ in size, so the lint would accept a word on
one machine and reject it on another. Point DICPATH at the output directory.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import urllib.request
import zipfile

LIMIT = 16 * 1024 * 1024

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
dictionaries = json.loads((root / "tools/lint/dictionaries.lock.json").read_text())
args.directory.mkdir(parents=True, exist_ok=True)
for name, dictionary in dictionaries.items():
    with urllib.request.urlopen(dictionary["url"], timeout=60) as response:
        package = response.read(LIMIT + 1)
    if len(package) > LIMIT:
        raise SystemExit(f"The package for {name} is larger than {LIMIT} bytes.")
    digest = hashlib.sha256(package).hexdigest()
    if digest != dictionary["sha256"]:
        raise SystemExit(f"The checksum for {name} differs from dictionaries.lock.json.")
    with zipfile.ZipFile(io.BytesIO(package)) as archive:
        for member in (f"{name}.aff", f"{name}.dic"):
            (args.directory / member).write_bytes(archive.read(member))
    print(f"Verified {name}: {digest}")
