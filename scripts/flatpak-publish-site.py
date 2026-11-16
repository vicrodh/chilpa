#!/usr/bin/env python3
"""What publish-flatpak.yml publishes, taken from the verified mirror only.

    scripts/flatpak-publish-site.py agree CLIENT_REFS MIRROR_REFS
    scripts/flatpak-publish-site.py assemble MIRROR DEST

`agree` reads two listings of `REF COMMIT` lines: what a Flatpak client with
main's key reads from the repository (app and runtime refs, from
`flatpak remote-info --show-commit`), and what the untrusted, signature
checked ostree mirror pull resolved. Every app and runtime ref must be in
both, with the same commit; the mirror's other refs (appstream) are its own.

`assemble` writes DEST, a repository a client can install from, out of the
mirror alone: its objects, which the pull checked one by one and which are
only those the signed commits reach, its refs, and the summary and its
signature it verified. The config is written here. No static delta and no
summary index is published: neither is in a mirror, and a client reads the
signed summary instead (flatpak 1.18.4, common/flatpak-dir.c, the summary
index fetch falls back to it on not-found).
"""
import argparse
import re
import shutil
import sys
from pathlib import Path

OBJECT = re.compile(r"[0-9a-f]{2}/[0-9a-f]{62}\.(filez|file|dirtree|dirmeta|commit|commitmeta|commitmeta2)")
# A ref: path segments of these characters, none of them `.` or `..`.
_SEGMENT = r"(?!\.\.?(?:/|$))[A-Za-z0-9._-]+"
REF = re.compile(rf"{_SEGMENT}(/{_SEGMENT})*")
COMMIT = re.compile(r"[0-9a-f]{64}")
CONFIG = "[core]\nrepo_version=1\nmode=archive-z2\n\n[flatpak]\ntitle=Chilpa\ndefault-branch=stable\n"


class Mismatch(ValueError):
    pass


def listing(text):
    """`REF COMMIT` lines as a mapping; anything else is refused."""
    refs = {}
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        parts = line.split()
        if len(parts) != 2 or not REF.fullmatch(parts[0]) or not COMMIT.fullmatch(parts[1]):
            raise Mismatch(f"line {number} is not a ref and a commit: {line!r}")
        if parts[0] in refs:
            raise Mismatch(f"{parts[0]} is listed twice")
        refs[parts[0]] = parts[1]
    return refs


def agree(client, mirror):
    """Raises unless the client's app and runtime refs are the mirror's."""
    installable = lambda ref: ref.startswith(("app/", "runtime/"))
    theirs = {ref: commit for ref, commit in client.items() if installable(ref)}
    ours = {ref: commit for ref, commit in mirror.items() if installable(ref)}
    if not ours:
        raise Mismatch("the mirror holds no app or runtime ref")
    if set(theirs) != set(ours):
        raise Mismatch(f"a client sees {sorted(theirs)}, the verified mirror {sorted(ours)}")
    differ = [ref for ref in ours if theirs[ref] != ours[ref]]
    if differ:
        raise Mismatch("a client and the verified mirror disagree on " + ", ".join(
            f"{ref} ({theirs[ref]} against {ours[ref]})" for ref in differ))
    return len(ours)


def assemble(mirror, dest):
    """Copies the mirror's verified tree to DEST, which must not exist."""
    mirror, dest = Path(mirror), Path(dest)
    if dest.exists():
        raise Mismatch(f"the destination exists already: {dest}")
    for present in ("deltas", "delta-indexes", "summaries", "summary.idx", "summary.idx.sig"):
        target = mirror / present
        if target.is_file() or (target.is_dir() and any(target.iterdir())):
            raise Mismatch(f"the mirror holds {present}, which is never published")
    for needed in ("summary", "summary.sig"):
        if not (mirror / needed).is_file():
            raise Mismatch(f"the mirror has no {needed}")
    objects = []
    for path in sorted((mirror / "objects").rglob("*")):
        if path.is_symlink():
            raise Mismatch(f"a link in the mirror: {path}")
        if path.is_dir():
            continue
        inner = path.relative_to(mirror / "objects").as_posix()
        if not OBJECT.fullmatch(inner):
            raise Mismatch(f"not an object of the mirror: objects/{inner}")
        objects.append(inner)
    refs = []
    for path in sorted((mirror / "refs" / "heads").rglob("*")):
        if path.is_symlink():
            raise Mismatch(f"a link in the mirror: {path}")
        if path.is_file():
            inner = path.relative_to(mirror / "refs" / "heads").as_posix()
            if not REF.fullmatch(inner) or not COMMIT.fullmatch(path.read_text().strip()):
                raise Mismatch(f"not a ref of the mirror: refs/heads/{inner}")
            refs.append(inner)
    if not objects or not refs:
        raise Mismatch("the mirror holds no objects or no refs")
    dest.mkdir(parents=True)
    for inner in objects:
        (dest / "objects" / inner).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(mirror / "objects" / inner, dest / "objects" / inner)
    for inner in refs:
        (dest / "refs" / "heads" / inner).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(mirror / "refs" / "heads" / inner, dest / "refs" / "heads" / inner)
    for name in ("summary", "summary.sig"):
        shutil.copyfile(mirror / name, dest / name)
    (dest / "config").write_text(CONFIG)
    return len(objects), len(refs)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    agreeing = commands.add_parser("agree")
    agreeing.add_argument("client", type=Path)
    agreeing.add_argument("mirror", type=Path)
    assembling = commands.add_parser("assemble")
    assembling.add_argument("mirror", type=Path)
    assembling.add_argument("dest", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "agree":
            count = agree(listing(args.client.read_text()), listing(args.mirror.read_text()))
            print(f"flatpak-publish-site: a client and the verified mirror agree on {count} refs")
        else:
            objects, refs = assemble(args.mirror, args.dest)
            print(f"flatpak-publish-site: {objects} objects and {refs} refs of the verified mirror in {args.dest}")
    except (Mismatch, OSError) as problem:
        print(f"::error::{problem}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
