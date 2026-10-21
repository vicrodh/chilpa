import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Patterns"
import "../../qml/Components"

// F9: Refine goes to a window of its own and comes back. The area keeps one
// line to dock it, the window folds to its bar and back, and docking closes
// the window and brings the panel home.
TestCase {
    id: test
    name: "RefineWindow"
    when: windowShown
    visible: true
    width: 1100
    height: 800

    RefineArea { id: area; width: 1000; height: fit(500); refineMode: true; summary: "256 songs · 18 h" }

    function init() {
        QueryChips.refineOf = { jazz: { mode: "all", complex: null, rows: [{ id: "0", field: "genre", kind: "list", negated: false, values: ["Jazz"] }] } }
        Library.queryText = "jazz"
        area.detached = false
    }
    function windows() {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === "refineWindowLoader" && child.item) out.push(child.item); walk(child) } }
        walk(area)
        return out
    }

    function test_refine_leaves_and_docks_back() {
        tryVerify(() => findChild(area.item, "refineDetach") !== null, 2000)
        mouseClick(findChild(area.item, "refineDetach"))
        verify(area.detached)
        tryVerify(() => findChild(area.item, "dockButton") !== null, 2000)
        compare(area.height, 34, "the area keeps one line")
        tryCompare(windows(), "length", 1)
        const window = windows()[0]
        compare(window.title, "Refine")
        const panel = findChild(window.contentItem, "refinePanel")
        verify(panel.detached, "the window's panel offers no second window")
        const fold = findChild(window.contentItem, "refineFold")
        tryVerify(() => fold.mapToItem(null, 0, 0).x > 400, 2000, "the bar is laid out across the window")
        mouseClick(fold)
        verify(window.folded)
        compare(window.height, 34, "folded to its bar")
        mouseClick(findChild(window.contentItem, "refineFold"))
        verify(!window.folded)
        mouseClick(findChild(window.contentItem, "refineDock"))
        verify(!area.detached)
        tryVerify(() => windows().length === 0, 2000)
        tryVerify(() => findChild(area.item, "refineDetach") !== null, 2000)
    }
}
