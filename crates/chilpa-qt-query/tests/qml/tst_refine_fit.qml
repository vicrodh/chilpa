import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Patterns"

// The panel sizes to its rows up to the grip, and its rows scroll past it.
// The rows' height never depends on how many fit: a ListView builds only
// the delegates its height shows, so a height taken from its contentHeight
// went round in a binding loop with the real query in the window.
TestCase {
    id: test
    name: "RefineFit"
    when: windowShown
    visible: true
    width: 1100
    height: 800

    property real grip: 160
    Component { id: areaFactory; RefineArea { width: 1000; height: fit(test.grip); refineMode: true; summary: "256 songs · 18 h" } }

    function init() {
        // Per test function: set in initTestCase it only guards initTestCase.
        failOnWarning(/.*/)
        const rows = []
        for (let i = 0; i < 24; ++i) rows.push({ id: String(i), field: i % 2 ? "genre" : "artist", kind: "list", negated: false, values: ["Value " + i] })
        QueryChips.refineOf = { many: { mode: "all", complex: null, rows: rows } }
        Library.queryText = "many"
    }

    function test_many_rows_scroll_inside_the_grip_without_a_loop() {
        const area = createTemporaryObject(areaFactory, test)
        tryVerify(() => area.item && findChild(area.item, "refineRows").count === 24, 2000)
        const rows = findChild(area.item, "refineRows")
        tryCompare(area, "height", test.grip, 2000, "the grip caps the panel")
        verify(rows.interactive, "the rows scroll")
        const full = rows.contentHeight
        test.grip = 600
        tryCompare(area, "height", 600)
        compare(rows.contentHeight, full, "the rows' height does not depend on the grip")
        test.grip = 160
        tryCompare(area, "height", 160)
    }
}
