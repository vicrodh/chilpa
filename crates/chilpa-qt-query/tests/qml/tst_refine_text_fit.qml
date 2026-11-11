import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "../../qml/Patterns"

// Refine says every field, operator and value in each language without
// running into its neighbours, also a tenth wider than Inter sets it.
TestCase {
    id: test
    name: "RefineTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1100
    height: 900

    readonly property var every: ({ mode: "all", complex: null, rows: [
        { id: "0", field: "genre", kind: "list", negated: false, values: ["Jazz", "Soul"] },
        { id: "1", field: "year", kind: "range", from: 1960, to: 1969 },
        { id: "2", field: "artist", kind: "list", negated: true, values: ["Miles Davis"] },
        { id: "3", field: "composer", kind: "none" },
        { id: "4", field: "label", kind: "list", negated: false, values: ["Blue Note"] },
        { id: "5", field: "rating", kind: "rating", min: 4, max: 5 },
        { id: "6", field: "added", kind: "added", n: 6, unit: "w" },
        { id: "7", field: "album", kind: "words", values: ["blue"] },
        { id: "8", field: "title", kind: "words", values: ["night"] },
        { id: "9", field: "plays", kind: "range", from: 3, to: 0 },
        { id: "10", field: "type", kind: "list", negated: false, values: ["compilation"] },
        { id: "11", field: "quality", kind: "list", negated: true, values: ["lossy"] },
        { id: "12", field: "bits", kind: "range", from: 24, to: 24 },
        { id: "13", field: "rate", kind: "range", from: 88200, to: 192000 },
        { id: "14", group: true, mode: "any", rows: [
            { id: "14.0", field: "bits", kind: "range", from: 16, to: 0 },
            { id: "14.1", field: "rate", kind: "range", from: 0, to: 48000 } ] } ] })

    Component { id: panelFactory; RefinePanel { width: 1000; height: 860; summary: "256 songs · 18 h" } }
    Component { id: narrowFactory; RefinePanel { width: 560; height: 860; narrow: true; summary: "256 songs · 18 h" } }

    function init() {
        QueryChips.refineOf = { every: test.every }
        Library.queryText = "every"
    }
    function cleanup() { I18n.choose("en") }

    function test_wide_rows_data() {
        return TextAudit.cases()
    }
    function test_wide_rows(data) { fits(panelFactory, data) }
    function test_narrow_rows_data() { return test_wide_rows_data() }
    function test_narrow_rows(data) { fits(narrowFactory, data) }

    // The label column keeps the design's width while the names fit it, and
    // grows for the longest one the rows show; the operators stay in line.
    function test_the_label_column_follows_the_names() {
        QueryChips.refineOf = { genre: { mode: "all", complex: null, rows: [test.every.rows[0]] }, every: test.every }
        Library.queryText = "genre"
        I18n.choose("es")
        const panel = createTemporaryObject(panelFactory, test)
        tryVerify(() => all(panel, "conditionLabel").length === 1, 2000)
        compare(all(panel, "conditionLabel")[0].width, 104, "Género fits the design's column")
        Library.queryText = "every"
        tryVerify(() => all(panel, "conditionLabel").length === 16, 2000)
        tryVerify(() => all(panel, "conditionLabel").every(label => label.width > 104), 2000, "Profundidad de bits widens it")
        const operators = all(panel, "conditionOperator")
        verify(operators.length >= 14)
        const xs = operators.map(pill => Math.round(pill.mapToItem(panel, 0, 0).x))
        const lead = xs.filter(x => x === xs[0]).length
        verify(lead >= 13, "the top level operators share one column: " + xs.join(","))
    }

    function all(root, name) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === name) out.push(child); walk(child) } }
        walk(root)
        return out
    }

    function fits(factory, data) {
        I18n.choose(data.language)
        const panel = createTemporaryObject(factory, test)
        tryVerify(() => findChild(panel, "refineRows").count === 15, 2000)
        const found = TextAudit.check(test, panel, data.scale)
        compare(found.length, 0, found.join("\n"))
    }
}
