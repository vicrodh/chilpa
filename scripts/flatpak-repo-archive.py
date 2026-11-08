#!/usr/bin/env python3
"""Unpacks an OSTree repository archive, `repo/` at its top, as data.

A job that holds a key unpacks an archive another job or a release made, so
nothing in it is trusted: each member must be a plain file or folder under
repo/, with no absolute path, no `..` and no `.git` part, and each file must
be one an OSTree repository holds (its config, its signed summary, objects
and refs). The lock and the tmp, state and extensions folders are left out.
Static deltas and a summary index stop the unpacking: they are never made,
since nothing a publisher can verify covers them, and a client reads the
signed summary instead. Anything else stops it too. Files are written
without execute bits, never through a link, into a folder that must not
exist yet. Reads the archive whole before writing anything.

    scripts/flatpak-repo-archive.py ARCHIVE DEST
"""
import argparse
import re
import sys
import tarfile
from pathlib import Path, PurePosixPath

# Each file a served OSTree repository holds, relative to repo/.
ALLOWED = [re.compile(pattern) for pattern in (
    r"config",
    r"summary(\.sig)?",
    r"objects/[0-9a-f]{2}/[0-9a-f]{62}\.(filez|file|dirtree|dirmeta|commit|commitmeta|commitmeta2)",
    r"refs/heads/[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*",
)]
# What a repository keeps for itself and a copy does not need.
SKIPPED = re.compile(r"\.lock|(tmp|state|extensions)(/.*)?")
# What is never published, file or folder.
NEVER = re.compile(r"(deltas|delta-indexes|summaries)(/.*)?|summary\.idx(\.sig)?")


class Refused(ValueError):
    pass


def relative(name):
    """The member's path inside repo/, or None for repo/ itself."""
    path = PurePosixPath(name)
    if path.is_absolute():
        raise Refused(f"an absolute path: {name!r}")
    parts = path.parts
    if any(part in ("..", "") for part in parts):
        raise Refused(f"a path that leaves its folder: {name!r}")
    if any(part.lower() == ".git" for part in parts):
        raise Refused(f"a git folder: {name!r}")
    if not parts or parts[0] != "repo":
        raise Refused(f"a path outside repo/: {name!r}")
    inner = parts[1:]
    return "/".join(inner) if inner else None


def plan(archive):
    """The folders and files to write, checked before any is written."""
    folders, files = [], []
    for member in archive.getmembers():
        inner = relative(member.name)
        if inner is not None and SKIPPED.fullmatch(inner):
            continue
        if inner is not None and NEVER.fullmatch(inner):
            raise Refused(f"a static delta or a summary index, which is never published: {member.name!r}")
        if member.isdir():
            if inner is not None:
                folders.append(inner)
            continue
        if not member.isreg():
            raise Refused(f"not a plain file: {member.name!r}")
        if inner is None or not any(pattern.fullmatch(inner) for pattern in ALLOWED):
            raise Refused(f"not a file of an OSTree repository: {member.name!r}")
        if any(inner == seen for seen, _ in files):
            raise Refused(f"a second copy of a file: {member.name!r}")
        files.append((inner, member))
    if not any(inner == "config" for inner, _ in files):
        raise Refused("no repo/config: not an OSTree repository")
    return folders, files


def unpack(archive_path, dest):
    dest = Path(dest)
    if dest.exists():
        raise Refused(f"the destination exists already: {dest}")
    with tarfile.open(archive_path, mode="r:") as archive:
        folders, files = plan(archive)
        root = dest / "repo"
        root.mkdir(parents=True)
        for inner in folders:
            (root / inner).mkdir(parents=True, exist_ok=True)
        for inner, member in files:
            target = root / inner
            target.parent.mkdir(parents=True, exist_ok=True)
            source = archive.extractfile(member)
            # O_EXCL: never through a link, never over a file already written.
            with open(target, "xb") as output:
                while chunk := source.read(1 << 20):
                    output.write(chunk)
            target.chmod(0o644)
    return len(files)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("archive", type=Path)
    parser.add_argument("dest", type=Path)
    args = parser.parse_args()
    try:
        count = unpack(args.archive, args.dest)
    except (Refused, tarfile.TarError, FileExistsError) as problem:
        print(f"::error::{args.archive} is not unpacked: {problem}", file=sys.stderr)
        return 1
    print(f"flatpak-repo-archive: {count} files of {args.archive} in {args.dest / 'repo'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
