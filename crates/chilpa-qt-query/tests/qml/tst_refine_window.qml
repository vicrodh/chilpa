import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
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

    function test_opaque_resizable_and_preserved_geometry() {
        area.detached = true
        tryCompare(windows(), "length", 1)
        const window = windows()[0]
        const surface = findChild(window.contentItem, "refineWindowSurface")
        compare(window.color.a, 1)
        compare(surface.color.a, 1)
        compare(Theme.opaque(Qt.rgba(0.1, 0.2, 0.3, 0.2)).a, 1)
        const grips = findChild(window.contentItem, "refineWindowGrips")
        verify(grips !== null)
        tryCompare(grips, "width", window.width)
        tryCompare(grips, "height", window.height)
        const edges = [Qt.LeftEdge, Qt.RightEdge, Qt.TopEdge, Qt.BottomEdge, Qt.LeftEdge | Qt.TopEdge, Qt.RightEdge | Qt.TopEdge, Qt.LeftEdge | Qt.BottomEdge, Qt.RightEdge | Qt.BottomEdge]
        let requested = 0
        const requestedResize = edge => requested = edge
        grips.resizeAsked.connect(requestedResize)
        for (const edge of edges) {
            const grip = findChild(grips, "windowResize" + edge)
            verify(grip.enabled)
            mouseClick(grip, grip.width / 2, grip.height / 2)
            compare(requested, edge, "the resize gesture reaches the native window")
        }
        grips.resizeAsked.disconnect(requestedResize)
        window.width = window.minimumWidth + 20
        window.height = window.minimumHeight + 20
        const sizedHeight = window.height
        const rows = []
        for (let i = 0; i < 12; ++i) rows.push({ id: String(i), field: "genre", kind: "list", negated: false, values: ["Value " + i] })
        QueryChips.refineOf = { many: { mode: "all", complex: null, rows: rows } }
        Library.queryText = "many"
        const viewport = findChild(window.contentItem, "refineWindowViewport")
        tryVerify(() => viewport.contentHeight > viewport.height)
        compare(window.height, sizedHeight, "editing conditions preserves the user's window size")
        window.folded = true
        compare(window.height, 34)
        verify(!findChild(grips, "windowResize" + Qt.BottomEdge).enabled)
        verify(findChild(grips, "windowResize" + Qt.LeftEdge).enabled)
        window.folded = false
        compare(window.height, sizedHeight)
        verify(window.maximumHeight > window.height)
        window.dockAsked()
        tryCompare(windows(), "length", 0)
    }

    function test_docking_and_closing_in_one_tick() {
        const rows = []
        for (let i = 0; i < 8; ++i) rows.push({ id: String(i), field: "genre", kind: "list", negated: false, values: ["Value " + i] })
        QueryChips.refineOf = { many: { mode: "all", complex: null, rows: rows } }
        Library.queryText = "many"
        area.visible = true
        tryVerify(() => findChild(area.item, "refineDetach") !== null, 2000)
        mouseClick(findChild(area.item, "refineDetach"))
        tryCompare(windows(), "length", 1)
        windows()[0].dockAsked()
        area.visible = false
        wait(100)
        area.visible = true
        Library.queryText = "jazz"
        wait(100)
        tryVerify(() => findChild(area.item, "refineDetach") !== null, 2000)
    }
}
