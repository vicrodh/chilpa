#!/usr/bin/env python3
"""I download the pinned packaging tools through gh and verify every payload."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
tools = json.loads((root / "packaging/linux/tools.lock.json").read_text())
args.directory.mkdir(parents=True, exist_ok=True)
for name, tool in tools.items():
    path = args.directory / tool["asset"]
    if not path.exists():
        subprocess.run(["gh", "release", "download", tool["tag"], "--repo", tool["repository"],
                        "--pattern", tool["asset"], "--dir", str(args.directory)], check=True)
    digest = hashlib.file_digest(path.open("rb"), "sha256").hexdigest()
    if digest != tool["sha256"]:
        raise SystemExit(f"The checksum for {name} differs from tools.lock.json; I refuse to run it.")
    path.chmod(0o755)
    print(f"Verified {name}: {digest}")
