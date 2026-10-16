import QtQuick
import QtTest
import "../../qml/Primitives"

TestCase {
    name: "KeyedModel"
    when: windowShown
    visible: true
    width: 500; height: 400
    KeyedModel { id: rows }
    ListView {
        id: view
        anchors.fill: parent
        model: rows
        delegate: Item {
            required property var entry
            required property string key
            width: view.width; height: 80
        }
    }
    function init() {
        rows.items = [{ id: "first", art: "" }, { id: "second", art: "" }]
        tryCompare(rows, "count", 2)
        tryVerify(() => view.itemAtIndex(0) !== null)
    }
    function test_cover_and_late_modules_preserve_existing_delegates() {
        const first = view.itemAtIndex(0)
        rows.items = [{ id: "first", art: "cover.jpg" }, { id: "second", art: "" }, { id: "late", art: "" }]
        tryCompare(rows, "count", 3)
        compare(view.itemAtIndex(0), first)
        compare(first.entry.art, "cover.jpg")
        rows.items = [{ id: "late", art: "" }, { id: "first", art: "cover.jpg" }, { id: "second", art: "" }]
        tryVerify(() => rows.get(0).key === "late")
        tryVerify(() => view.itemAtIndex(1) === first, 1000, "moving a module preserves its mounted body")
    }
}
