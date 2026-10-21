import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Patterns"
import "../../qml/Components"

// F3: adding is one step. A field chip opens its values inside the panel
// (a click on the row includes, − keeps out, again takes it back), years
// offer decades and a free range, stars their kinds, Added its spans; what
// is typed is looked for in every field, and Enter takes the first match.
TestCase {
    id: test
    name: "RefinePick"
    when: windowShown
    visible: true
    width: 1100
    height: 800

    RefinePanel { id: panel; width: 1000; height: 760; summary: "256 songs · 18 h" }

    function init() {
        QueryChips.refineOf = { jazz: { mode: "all", complex: null, rows: [
            { id: "0", field: "genre", kind: "list", negated: false, values: ["Jazz"] },
            { id: "1", field: "genre", kind: "list", negated: true, values: ["Pop"] } ] } }
        QueryChips.facets = { genre: [{ key: "", label: "All", meta: "", all: true }, { key: "Jazz", label: "Jazz", meta: "12", all: false },
                                      { key: "Pop", label: "Pop", meta: "3", all: false }, { key: "Soul", label: "Soul", meta: "4", all: false }] }
        QueryChips.suggestions = { groups: [{ field: "artist", items: [{ value: "Thelonious Monk", count: 17, state: "none" }] },
                                            { field: "composer", items: [{ value: "Thelonious Monk", count: 14, state: "none" }] }],
                                   words: { anywhere: 269, title: 101, album: 101 } }
        QueryChips.refinedText = "edited"
        QueryChips.lastRefineEdit = ({})
        Library.queryText = "jazz"
        panel.picking = ""
        panel.target = ""
    }
    function all(name) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === name) out.push(child); walk(child) } }
        walk(panel)
        return out
    }
    function chip(field) { return all("fieldChip").find(c => c.text === field) }
    function rowFor(text) { return all("pickValues").length ? findValue(text) : null }
    function findValue(text) {
        const list = all("pickValues")[0]
        for (let i = 0; i < list.count; ++i) { const item = list.itemAtIndex(i); if (item && item.text === text) return item }
        return null
    }

    function test_a_field_chip_opens_its_values_in_place() {
        mouseClick(chip("Genre"))
        compare(panel.picking, "genre")
        verify(chip("Genre").active, "its chip is inverted while open")
        tryVerify(() => findValue("Soul") !== null, 2000)
        compare(findValue("Jazz").mark, "include")
        compare(findValue("Pop").mark, "exclude")
        compare(findValue("Soul").count, "4")
        mouseClick(findValue("Soul"))
        compare(QueryChips.lastRefineEdit, { op: "value", field: "genre", value: "Soul", state: "include" })
        Library.queryText = "jazz"
        tryVerify(() => findValue("Soul") !== null, 2000)
        mouseClick(findChild(findValue("Soul"), "pickExclude"))
        compare(QueryChips.lastRefineEdit, { op: "value", field: "genre", value: "Soul", state: "exclude" })
        Library.queryText = "jazz"
        tryVerify(() => findValue("Jazz") !== null, 2000)
        mouseClick(findValue("Jazz"))
        compare(QueryChips.lastRefineEdit.state, "none", "a second click takes it back")
        mouseClick(all("pickDone")[0])
        compare(panel.picking, "")
    }

    function pills() {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (String(child).indexOf("Pill") === 0) out.push(child); walk(child) } }
        walk(panel)
        return out
    }
    function test_years_stars_and_recency() {
        panel.picking = "year"
        tryVerify(() => pills().length === 9, 2000)
        mouseClick(pills().find(p => p.text === "1960s"))
        compare(QueryChips.lastRefineEdit, { op: "range", field: "year", from: 1960, to: 1969 })
        Library.queryText = "jazz"
        panel.picking = "added"
        tryVerify(() => pills().some(p => p.text === "6 weeks"), 2000)
        mouseClick(pills().find(p => p.text === "6 weeks"))
        compare(QueryChips.lastRefineEdit, { op: "added", n: 6, unit: "w" })
        Library.queryText = "jazz"
        panel.picking = "title"
        tryVerify(() => findChild(panel, "pickWords") !== null, 2000)
        const words = findChild(panel, "pickWords")
        words.forceActiveFocus()
        words.text = "blue"
        keyClick(Qt.Key_Return)
        compare(QueryChips.lastRefineEdit, { op: "words", field: "title", word: "blue" })
    }

    function test_typing_finds_across_fields_and_enter_takes_the_first() {
        const input = findChild(panel, "refineAddInput")
        input.forceActiveFocus()
        input.text = "monk"
        tryVerify(() => all("suggestion").length === 2, 2000)
        compare(QueryChips.lastTyped, "monk")
        keyClick(Qt.Key_Return)
        compare(QueryChips.lastRefineEdit, { op: "value", field: "artist", value: "Thelonious Monk", state: "include" })
        Library.queryText = "jazz"
        input.text = "monk"
        tryVerify(() => all("suggestion").length === 2, 2000)
        keyClick(Qt.Key_Return, Qt.ShiftModifier)
        compare(QueryChips.lastRefineEdit.state, "exclude")
        input.text = "monk"
        tryVerify(() => all("suggestion").length === 2 && all("suggestionWords").length === 1, 2000)
        const words = all("suggestionWords")[0]
        waitForRendering(words)
        mouseClick(words)
        compare(QueryChips.lastRefineEdit, { op: "words", field: "words", word: "monk" })
    }
}
