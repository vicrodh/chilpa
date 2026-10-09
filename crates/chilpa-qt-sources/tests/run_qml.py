#!/usr/bin/env python3
"""Run a QML test with every shell crate's tree staged locally.

The trees import each other by their resource path, which only exists
inside the built binary. This copies them to a scratch directory, points
those imports at the copies, and runs the named tests there with every
crate's test doubles on the import path.

    run_qml.py <name> [<name> ...]     # tst_<name>.qml, in whichever crate
"""
import pathlib
import shutil
import subprocess
import sys
import tempfile

root = pathlib.Path(__file__).resolve().parents[3]
# Each shell crate and where its QML sits in the binary's resources.
CRATES = {
    "chilpa-qt-kit": "qrc:/qt/qml/app/chilpa/kit/qml/",
    "chilpa-qt": "qrc:/qt/qml/app/chilpa/qml/",
    "chilpa-qt-sources": "qrc:/qt/qml/app/chilpa/sources/qml/",
    "chilpa-qt-controls": "qrc:/qt/qml/app/chilpa/controls/qml/",
    "chilpa-qt-lyrics": "qrc:/qt/qml/app/chilpa/lyrics/qml/",
    "chilpa-qt-metadata": "qrc:/qt/qml/app/chilpa/metadata/qml/",
    "chilpa-qt-gallery": "qrc:/qt/qml/app/chilpa/gallery/qml/",
    "chilpa-qt-folders": "qrc:/qt/qml/app/chilpa/folders/qml/",
    "chilpa-qt-ambient": "qrc:/qt/qml/app/chilpa/ambient/qml/",
    "chilpa-qt-mini": "qrc:/qt/qml/app/chilpa/mini/qml/",
    "chilpa-qt-immersive": "qrc:/qt/qml/app/chilpa/immersive/qml/",
    "chilpa-qt-waveform": "qrc:/qt/qml/app/chilpa/waveform/qml/",
    "chilpa-qt-artwork": "qrc:/qt/qml/app/chilpa/artwork/qml/",
    "chilpa-qt-about": "qrc:/qt/qml/app/chilpa/about/qml/",
}

with tempfile.TemporaryDirectory(prefix="chilpa-shared-qml-") as directory:
    stage = pathlib.Path(directory)
    for crate in CRATES:
        for part in ["qml", "tests/qml"]:
            if (root / "crates" / crate / part).is_dir():
                shutil.copytree(root / "crates" / crate / part, stage / "crates" / crate / part)
    shutil.copytree(root / "crates/chilpa-qt-kit/assets", stage / "crates/chilpa-qt-kit/assets")
    for path in stage.rglob("*.qml"):
        text = path.read_text()
        for crate, prefix in CRATES.items():
            text = text.replace(prefix, (stage / "crates" / crate / "qml").as_uri() + "/")
        path.write_text(text)
    imports = [str(found) for found in sorted(stage.glob("crates/*/tests/qml/imports"))]
    for test in sys.argv[1:]:
        candidates = list(stage.glob("crates/*/tests/qml/tst_" + test + ".qml"))
        if len(candidates) != 1:
            raise SystemExit("Expected one test: " + test)
        command = ["/usr/lib64/qt6/bin/qmltestrunner", "-platform", "offscreen"]
        for found in imports:
            command += ["-import", found]
        subprocess.run(command + ["-input", str(candidates[0])], check=True)
