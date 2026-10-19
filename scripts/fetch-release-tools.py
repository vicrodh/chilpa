#!/usr/bin/env python3
"""I download the pinned packaging tools through gh and verify every payload."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
from release_platform import architecture

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
parser.add_argument("--arch", choices=("x86_64", "aarch64"), default=architecture())
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
tools = json.loads((root / "packaging/linux/tools.lock.json").read_text())[args.arch]
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
    if "executable" in tool:
        with tarfile.open(path) as archive:
            member = archive.getmember(tool["executable"])
            if not member.isfile():
                raise SystemExit("The packaging archive must contain a regular executable.")
            executable = args.directory / tool["executable"]
            with archive.extractfile(member) as source, executable.open("wb") as destination:
                destination.write(source.read())
            executable.chmod(0o755)
    print(f"Verified {name}: {digest}")
