"""Release identity and update channels shared by packaging and CI."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib


_NUMBER = r"(?:0|[1-9][0-9]*)"
_IDENTIFIER = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
_VERSION = re.compile(rf"{_NUMBER}\.{_NUMBER}\.{_NUMBER}(?:-{_IDENTIFIER}(?:\.{_IDENTIFIER})*)?")
_REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")


def release_version(version):
    if not _VERSION.fullmatch(version):
        raise ValueError("I require X.Y.Z or X.Y.Z-prerelease without leading numeric zeroes.")
    return {"version": version, "tag": "v" + version, "prerelease": "-" in version}


def update_information(repository, version, architecture="x86_64"):
    if not _REPOSITORY.fullmatch(repository):
        raise ValueError("I require a repository in owner/name form.")
    identity = release_version(version)
    if architecture not in ("x86_64", "aarch64"):
        raise ValueError("Unsupported AppImage architecture.")
    owner, name = repository.split("/")
    # Prerelease fallbacks stay pinned; the host selects newer releases explicitly.
    channel = identity["tag"] if identity["prerelease"] else "latest"
    return f"gh-releases-zsync|{owner}|{name}|{channel}|Chilpa-*-{architecture}.AppImage.zsync"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version")
    parser.add_argument("--github-env", type=Path)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    version = args.version or tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    try:
        identity = release_version(version)
    except ValueError as error:
        parser.error(str(error))
    if os.environ.get("RELEASE_EVENT") == "push" and os.environ.get("RELEASE_REF") != "refs/tags/" + identity["tag"]:
        parser.error("The release tag and workspace version differ.")
    if args.github_env:
        epoch = subprocess.check_output(["git", "show", "-s", "--format=%ct", "HEAD"], cwd=root, text=True).strip()
        with args.github_env.open("a") as output:
            output.write(f"RELEASE_VERSION={version}\nSOURCE_DATE_EPOCH={epoch}\n")
    if args.github_output:
        with args.github_output.open("a") as output:
            output.write(f"version={version}\nprerelease={str(identity['prerelease']).lower()}\n")
    print(json.dumps(identity))


if __name__ == "__main__":
    main()
