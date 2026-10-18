#!/usr/bin/env python3
"""Download the pinned hunspell dictionaries the language lint reads, and verify them.

A distribution's dictionaries differ in size, so the lint would accept a word on
one machine and reject it on another. Point DICPATH at the output directory. A
package already in it is verified and reused, so a cached directory needs no
network.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import time
import urllib.request
import zipfile

LIMIT = 16 * 1024 * 1024
ATTEMPTS = 3


def download(url):
    for attempt in range(1, ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                package = response.read(LIMIT + 1)
            break
        except OSError as error:
            if attempt == ATTEMPTS:
                raise
            print(f"Attempt {attempt} for {url} failed: {error}")
            time.sleep(10 * attempt)
    if len(package) > LIMIT:
        raise SystemExit(f"{url} is larger than {LIMIT} bytes.")
    return package


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
dictionaries = json.loads((root / "tools/lint/dictionaries.lock.json").read_text())
args.directory.mkdir(parents=True, exist_ok=True)
for name, dictionary in dictionaries.items():
    path = args.directory / f"{name}.oxt"
    if not path.exists():
        path.write_bytes(download(dictionary["url"]))
    package = path.read_bytes()
    digest = hashlib.sha256(package).hexdigest()
    if digest != dictionary["sha256"]:
        path.unlink()
        raise SystemExit(f"The checksum for {name} differs from dictionaries.lock.json.")
    with zipfile.ZipFile(io.BytesIO(package)) as archive:
        for member in (f"{name}.aff", f"{name}.dic"):
            (args.directory / member).write_bytes(archive.read(member))
    print(f"Verified {name}: {digest}")
