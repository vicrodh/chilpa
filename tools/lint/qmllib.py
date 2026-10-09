"""What the QML checks share: where the tree is, its layers, and a scanner
that tells code from comments and strings.

The layers, lowest first. A file belongs to the directory it sits in right
under `qml/`; a file at the top of `qml/` is the window itself.
"""
import re
import subprocess
from pathlib import Path

ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
    ).stdout.strip()
)
QML = ROOT / "crates/chilpa-qt/qml"
# The kit is the tree every other one stands on: the theme and the
# primitives. Its layers sit below the same layers of any other tree.
KIT = ROOT / "crates/chilpa-qt-kit/qml"
# The shell is several QML trees with the same layers: the kit, the window's,
# and one per feature crate. They import each other's lower layers by their
# resource path, which `resolve_import` maps back.
TREES = [KIT, QML, ROOT / "crates/chilpa-qt-sources/qml", ROOT / "crates/chilpa-qt-controls/qml", ROOT / "crates/chilpa-qt-lyrics/qml",
         ROOT / "crates/chilpa-qt-metadata/qml", ROOT / "crates/chilpa-qt-gallery/qml",
         ROOT / "crates/chilpa-qt-folders/qml", ROOT / "crates/chilpa-qt-ambient/qml", ROOT / "crates/chilpa-qt-mini/qml",
         ROOT / "crates/chilpa-qt-immersive/qml"]
# Where each tree sits in the binary's resources, for imports across them.
RESOURCES = {"qrc:/qt/qml/app/chilpa/kit/qml/": TREES[0], "qrc:/qt/qml/app/chilpa/qml/": TREES[1],
             "qrc:/qt/qml/app/chilpa/sources/qml/": TREES[2], "qrc:/qt/qml/app/chilpa/controls/qml/": TREES[3],
             "qrc:/qt/qml/app/chilpa/lyrics/qml/": TREES[4], "qrc:/qt/qml/app/chilpa/metadata/qml/": TREES[5],
             "qrc:/qt/qml/app/chilpa/gallery/qml/": TREES[6], "qrc:/qt/qml/app/chilpa/folders/qml/": TREES[7],
             "qrc:/qt/qml/app/chilpa/ambient/qml/": TREES[8],
             "qrc:/qt/qml/app/chilpa/mini/qml/": TREES[9],
             "qrc:/qt/qml/app/chilpa/immersive/qml/": TREES[10]}

ORDER = {"Theme": 0, "Primitives": 1, "Components": 2, "Patterns": 3, "Screens": 4, "Main": 5}
# The Gallery is a second window that shows every piece; it sits beside the
# layers, not on top of them, and may use anything but a screen.
GALLERY = "Gallery"


def files():
    return sorted(p for tree in TREES for p in tree.rglob("*.qml"))


def tree_of(path: Path) -> Path:
    return next(tree for tree in TREES if path.resolve().is_relative_to(tree.resolve()))


def resolve_import(path: Path, target: str):
    """Where an import points, as (tree, path inside it), or None when it
    leaves every tree."""
    prefix = next((p for p in RESOURCES if target.startswith(p)), None)
    where = RESOURCES[prefix] / target[len(prefix):] if prefix else path.parent / target
    where = where.resolve()
    for tree in TREES:
        if where.is_relative_to(tree.resolve()):
            return tree, where.relative_to(tree.resolve())
    return None


def layer_of(path: Path):
    rel = path.relative_to(tree_of(path))
    if len(rel.parts) == 1:
        return "Main"
    head = rel.parts[0]
    return head if head in ORDER or head == GALLERY else None


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


def pieces(src: str):
    """(kind, text) with kind in code | comment | string, in order."""
    out, i, n, start = [], 0, len(src), 0
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            out.append(("code", src[start:i]))
            j = src.find("\n", i)
            j = n if j < 0 else j
            out.append(("comment", src[i:j]))
            i = start = j
        elif src.startswith("/*", i):
            out.append(("code", src[start:i]))
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append(("comment", src[i:j]))
            i = start = j
        elif c in "\"'`":
            out.append(("code", src[start:i]))
            j = i + 1
            while j < n and src[j] != c:
                j += 2 if src[j] == "\\" else 1
            out.append(("string", src[i : j + 1]))
            i = start = j + 1
        else:
            i += 1
    out.append(("code", src[start:]))
    return out


def without_comments(src: str) -> str:
    """The source with every comment blanked out, line numbers kept."""
    return "".join(re.sub(r"[^\n]", " ", t) if k == "comment" else t for k, t in pieces(src))


def code_only(src: str) -> str:
    """Comments and strings blanked out, line numbers kept. Strings become
    empty quotes so an assignment still reads as one."""
    out = []
    for kind, text in pieces(src):
        if kind == "code":
            out.append(text)
        elif kind == "string":
            out.append('""' + re.sub(r"[^\n]", "", text[2:]))
        else:
            out.append(re.sub(r"[^\n]", " ", text))
    return "".join(out)


def effective_lines(src: str) -> int:
    return sum(1 for line in without_comments(src).split("\n") if line.strip())
