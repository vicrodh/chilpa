#!/usr/bin/env python3
"""Run shared host/source QML tests with production resource imports staged locally."""
import pathlib
import shutil
import subprocess
import sys
import tempfile

root = pathlib.Path(__file__).resolve().parents[3]
with tempfile.TemporaryDirectory(prefix="chilpa-shared-qml-") as directory:
    stage = pathlib.Path(directory)
    for crate in ["chilpa-qt", "chilpa-qt-sources"]:
        for part in ["qml", "tests/qml"]:
            shutil.copytree(root / "crates" / crate / part, stage / "crates" / crate / part)
    shutil.copytree(root / "crates/chilpa-qt/assets", stage / "crates/chilpa-qt/assets")
    for path in stage.rglob("*.qml"):
        text = path.read_text()
        for crate, prefix in [("chilpa-qt", "qrc:/qt/qml/com/blitzfc/chilpa/qml/"), ("chilpa-qt-sources", "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/")]:
            text = text.replace(prefix, (stage / "crates" / crate / "qml").as_uri() + "/")
        path.write_text(text)
    imports = stage / "crates/chilpa-qt/tests/qml/imports"
    for test in sys.argv[1:]:
        candidates = list(stage.glob("crates/*/tests/qml/tst_" + test + ".qml"))
        if len(candidates) != 1:
            raise SystemExit("Expected one test: " + test)
        subprocess.run(["/usr/lib64/qt6/bin/qmltestrunner", "-platform", "offscreen", "-import", str(imports), "-input", str(candidates[0])], check=True)
