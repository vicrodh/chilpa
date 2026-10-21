import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Components"

// F1: with Refine closed the crumb line carries the whole query, one chip
// per condition (glyph, field, values, x), as many as fit and then
// "+N more"; a chip or "+N more" opens Refine, an x takes it away.
TestCase {
    id: test
    name: "QueryChips"
    when: windowShown
    visible: true
    width: 1100
    height: 100

    readonly property var full: ({ mode: "all", complex: null, rows: [
        { id: "0", field: "genre", kind: "list", negated: false, values: ["Jazz", "Soul"] },
        { id: "1", field: "genre", kind: "list", negated: true, values: ["Smooth Jazz"] },
        { id: "2", field: "year", kind: "range", from: 1960, to: 1969 },
        { id: "3", field: "rating", kind: "rating", min: 4, max: 5 },
        { id: "4", field: "added", kind: "added", n: 6, unit: "w" },
        { id: "5", field: "label", kind: "none" },
        { id: "6", field: "title", kind: "words", values: ["blue"] } ] })
    property string asked: ""
    property int opened: 0
    QueryChipRow { id: row; width: test.width; query: "full"; onEdited: (query) => test.asked = query; onOpenAsked: test.opened += 1 }

    function init() {
        QueryChips.refineOf = { full: test.full }
        row.width = test.width
        asked = ""
        opened = 0
    }
    function chips() {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === "summaryChip") out.push(child); walk(child) } }
        walk(row)
        return out
    }

    function test_each_condition_says_its_field_and_values() {
        const all = chips()
        compare(all.length, 7)
        const values = all.map(chip => findChild(chip, "summaryChipValue").text)
        compare(values, ["Jazz, Soul", "Smooth Jazz", "1960 – 1969", "≥ 4★", "in the last 6 weeks", "has none", "“blue”"])
    }

    function test_what_does_not_fit_is_counted_and_opens_refine() {
        row.width = 420
        tryVerify(() => findChild(row, "summaryMore").visible)
        const shown = chips().filter(chip => chip.visible).length
        verify(shown > 0 && shown < 7, "some chips, not all")
        compare(findChild(row, "summaryMore").text, "+" + (7 - shown) + " more")
        mouseClick(findChild(row, "summaryMore"))
        compare(test.opened, 1)
    }

    function test_a_chip_opens_refine_and_its_x_removes_it() {
        const chip = chips()[2]
        tryVerify(() => chip.visible, 1000, "with room for all, every chip shows")
        mouseClick(chip, 4, chip.height / 2)
        compare(test.opened, 1)
        QueryChips.refinedText = "without year"
        mouseClick(findChild(chip, "summaryChipRemove"))
        compare(QueryChips.lastRefineEdit, { op: "remove", id: "2" })
        compare(test.asked, "without year")
    }

    function test_an_empty_query_shows_no_line() {
        row.query = ""
        verify(!row.visible)
        row.query = "full"
        verify(row.visible)
    }
}
