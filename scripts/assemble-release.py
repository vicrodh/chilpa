#!/usr/bin/env python3
"""I require every desktop platform before assembling a publishable release."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from release_platform import linux_assets, macos_assets
from release_version import release_version


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def assemble(source, output, version, commit):
    release_version(version)
    expected = []
    for system, assets in (("linux", linux_assets), ("macos", macos_assets)):
        for arch in ("x86_64", "aarch64"):
            folder = source / f"chilpa-{system}-{arch}"
            names = assets(version, arch)
            sums = folder / f"SHA256SUMS-{system}-{arch}"
            records = {}
            for line in sums.read_text().splitlines():
                checksum, name = line.split("  ", 1)
                if name in records or Path(name).name != name:
                    raise ValueError("Ambiguous checksum filename")
                records[name] = checksum
            for name in names:
                path = folder / name
                if not path.is_file() or path.is_symlink() or path.stat().st_size == 0 or records.get(name) != digest(path):
                    raise ValueError(f"Missing or unverified release asset: {path}")
                expected.append(path)
            manifest = json.loads((folder / f"MANIFEST-{system}-{arch}.json").read_text())
            if (manifest["version"], manifest["source_commit"], manifest["platform"]) != (version, commit, f"{system}-{arch}"):
                raise ValueError(f"Mismatched release identity: {folder}")
    output.mkdir(parents=True, exist_ok=True)
    for path in expected:
        shutil.copy2(path, output / path.name)
    root = Path(__file__).resolve().parents[1]
    shutil.copy2(root / "scripts/install.sh", output / "install.sh")
    shutil.copy2(root / "CHANGELOG.md", output / "RELEASE-NOTES.md")
    (output / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.name}\n" for path in sorted(output.iterdir()) if path.name != "SHA256SUMS"))
    print(f"I verified {len(expected)} platform assets from {commit}.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    assemble(args.input, args.output, args.version, args.commit)
