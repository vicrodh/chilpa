#!/usr/bin/env python3
"""I install and update the actual release payload in an isolated user directory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import zipfile

from release_platform import architecture
from release_version import release_version


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--package", type=Path, required=True)
parser.add_argument("--version", required=True)
parser.add_argument("--arch", choices=("x86_64", "aarch64"), required=True)
args = parser.parse_args()
identity = release_version(args.version)
assert architecture() == args.arch
package = args.package.resolve(strict=True)
root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="chilpa-installer-smoke-") as directory:
    work = Path(directory)
    user = work / "user with spaces"
    user.mkdir()
    tools = work / "bin"
    tools.mkdir()
    # I replace only the network transport; validation and installation run unchanged.
    fetcher = tools / "curl"
    fetcher.write_text("#!" + sys.executable + "\n" + '''import os, pathlib, shutil, sys
args = sys.argv[1:]
metadata = args[-1].startswith("https://api.github.com/")
source = os.environ["CHILPA_INSTALL_TEST_RELEASE" if metadata else "CHILPA_INSTALL_TEST_PACKAGE"]
output = pathlib.Path(args[args.index("--output") + 1])
shutil.copyfile(source, output)
if not metadata and os.environ.get("CHILPA_INSTALL_TEST_CORRUPT") == "1":
    with output.open("r+b") as stream:
        stream.write(b"corrupt")
''')
    fetcher.chmod(0o755)
    release = work / "release.json"
    release.write_text(json.dumps(dict(tag_name=identity["tag"], draft=False, prerelease=identity["prerelease"],
        assets=[dict(name=package.name, size=package.stat().st_size, digest="sha256:" + digest(package),
            browser_download_url=f"https://github.com/vicrodh/chilpa/releases/download/{identity['tag']}/{package.name}")])))
    environment = dict(os.environ, HOME=str(user), PATH=str(tools) + os.pathsep + os.environ["PATH"],
        CHILPA_INSTALL_TEST_RELEASE=str(release), CHILPA_INSTALL_TEST_PACKAGE=str(package))
    command = ["bash", str(root / "scripts/install.sh")]
    if identity["prerelease"]:
        command.append("--version=" + args.version)
    if platform.system() == "Darwin":
        installed = user / "Applications/Chilpa.app/Contents/MacOS/chilpa-qt"
        with zipfile.ZipFile(package) as archive, archive.open("Chilpa.app/Contents/MacOS/chilpa-qt") as binary:
            expected = hashlib.file_digest(binary, "sha256").hexdigest()
    else:
        installed = user / ".local/opt/chilpa/Chilpa.AppImage"
        expected = digest(package)
    for action in ("install", "update"):
        result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=120)
        print(result.stdout, end="")
        if result.returncode:
            print(result.stderr, file=sys.stderr)
            raise SystemExit(f"The real package failed its {action} smoke.")
        assert digest(installed) == expected
    environment["CHILPA_INSTALL_TEST_CORRUPT"] = "1"
    result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=120)
    assert result.returncode != 0 and "checksum mismatch" in result.stderr
    assert digest(installed) == expected
print(json.dumps(dict(platform=platform.system(), arch=args.arch, version=args.version,
    installed=True, updated=True, corrupt_download_preserved_installation=True)))
