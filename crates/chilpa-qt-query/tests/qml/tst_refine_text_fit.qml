import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "../../qml/Patterns"
import "../../qml/Components"

// Refine says every field, operator and value in each language without
// running into its neighbours, also a tenth wider than Inter sets it: its
// rows, each kind of value picker, the suggestions, the summary chips on the
// crumb line, the filter columns and the saved queries.
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
    Component { id: chipsFactory; QueryChipRow { width: 700; query: "every" } }
    Component { id: panesFactory; LibraryPanes { width: 1000; height: 300; shown: ["year", "genre", "label", "composer", "rating", "bits", "rate", "artist", "album"] } }
    Component { id: savedFactory; SavedQueriesDialog { parent: test } }

    function init() {
        QueryChips.refineOf = { every: test.every }
        QueryChips.facets = { genre: [{ key: "", label: "All", meta: "", all: true }, { key: "Jazz", label: "Jazz", meta: "1,940", all: false },
                                      { key: "Soul", label: "Soul", meta: "412", all: false }] }
        QueryChips.suggestions = { groups: [{ field: "artist", items: [{ value: "Thelonious Monk", count: 17, state: "none" }] },
                                            { field: "composer", items: [{ value: "Thelonious Monk", count: 14, state: "include" }] }],
                                   words: { anywhere: 269, title: 101, album: 101 } }
        QueryChips.answers = true
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

    // Each kind of picker: values, decades and years, stars, spans, audio
    // and words, and what typing suggests.
    function test_pickers_data() { return TextAudit.cases() }
    function test_pickers(data) {
        I18n.choose(data.language)
        const panel = createTemporaryObject(panelFactory, test)
        tryVerify(() => all(panel, "conditionLabel").length === 16, 2000)
        for (const field of ["genre", "year", "rating", "added", "type", "quality", "bits", "rate", "title"]) {
            panel.picking = field
            const found = TextAudit.check(test, panel, data.scale)
            compare(found.length, 0, field + ":\n" + found.join("\n"))
        }
        // The fields past the ninth, scrolled into view.
        const fields = findChild(panel, "pickFields")
        fields.contentY = fields.contentHeight - fields.height
        for (const field of ["bits", "rate"]) {
            panel.picking = field
            const found = TextAudit.check(test, panel, data.scale)
            compare(found.length, 0, field + " scrolled:\n" + found.join("\n"))
        }
        panel.picking = ""
        findChild(panel, "refineAddInput").text = "monk"
        const found = TextAudit.check(test, panel, data.scale)
        compare(found.length, 0, "suggestions:\n" + found.join("\n"))
    }

    function test_summary_chips_data() { return TextAudit.cases() }
    function test_summary_chips(data) {
        I18n.choose(data.language)
        const chips = createTemporaryObject(chipsFactory, test)
        tryVerify(() => all(chips, "summaryChip").length === 15, 2000)
        const found = TextAudit.check(test, chips, data.scale)
        compare(found.length, 0, found.join("\n"))
    }

    function test_filter_columns_data() { return TextAudit.cases() }
    function test_filter_columns(data) {
        I18n.choose(data.language)
        const panes = createTemporaryObject(panesFactory, test)
        waitForRendering(panes)
        const found = TextAudit.check(test, panes, data.scale)
        compare(found.length, 0, found.join("\n"))
    }

    function test_saved_queries_data() { return TextAudit.cases() }
    function test_saved_queries(data) {
        I18n.choose(data.language)
        SavedQueries.savedEntries = [{ id: "q1", name: "Evening collection", expression: '(genre:jazz | genre:soul) year:1960..1989 -title:"Live"' }]
        SavedQueries.publishSaved()
        const dialog = createTemporaryObject(savedFactory, test)
        for (const mode of ["list", "save"]) {
            dialog.mode = mode
            dialog.open()
            tryCompare(dialog, "opened", true)
            const found = TextAudit.check(test, dialog.contentItem, data.scale)
            compare(found.length, 0, mode + ":\n" + found.join("\n"))
            dialog.close()
            tryCompare(dialog, "visible", false)
        }
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
