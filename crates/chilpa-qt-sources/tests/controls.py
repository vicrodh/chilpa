#!/usr/bin/env python3
"""Exercise source controls offscreen with the host's singleton test doubles."""
import pathlib
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[3]
with tempfile.TemporaryDirectory(prefix="chilpa-source-controls-") as directory:
    stage = pathlib.Path(directory)
    # Only resource imports change; I test the production component itself.
    source = (root / "crates/chilpa-qt-sources/qml/Patterns/SourceLibraryControls.qml").read_text()
    source = source.replace("qrc:/qt/qml/com/blitzfc/chilpa/qml/", (root / "crates/chilpa-qt/qml").as_uri() + "/")
    (stage / "SourceLibraryControls.qml").write_text(source)
    (stage / "tst_controls.qml").write_text('''import QtQuick
import QtTest
import com.blitzfc.chilpa
import "."
TestCase {
    name: "SourceLibraryControls"; when: windowShown
    SourceLibraryControls { id: controls; source: "fixture section:albums"; entity: "album" }
    function test_preferences_follow_source_and_entity_routes() {
        controls.children[0].picked("label")
        controls.children[1].picked("decade")
        controls.children[2].clicked()
        compare(controls.sort, "label:desc")
        controls.source = "another section:artists"; controls.entity = "artist"
        compare(controls.order, "default"); compare(controls.grouping, ""); compare(controls.descending, false)
        compare(controls.groups.length, 2)
        controls.source = "fixture section:albums"; controls.entity = "album"
        compare(controls.sort, "label:desc"); compare(controls.grouping, "decade")
    }
}''')
    result = subprocess.run([
        "/usr/lib64/qt6/bin/qmltestrunner", "-platform", "offscreen", "-import",
        str(root / "crates/chilpa-qt/tests/qml/imports"), "-input", str(stage / "tst_controls.qml"),
    ], check=False)
    raise SystemExit(result.returncode)
