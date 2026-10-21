import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Patterns"
import "../../qml/Components"

// F5, F6, F9: a query too deep for rows stays as text and opens the console;
// what the engine read on its own shows under "Read as" and each can go;
// the console line is hidden until asked for, marks where a typed query
// stops reading, and applies one that reads; with no tracks left the
// records offer to take one condition away.
TestCase {
    id: test
    name: "RefineMore"
    when: windowShown
    visible: true
    width: 1100
    height: 1000

    RefinePanel { id: panel; width: 1000; height: 700; summary: "24 songs · 2 h" }
    ZeroResults { id: zero; y: 720; width: 1000; query: "zero"; active: true }

    function init() {
        QueryChips.refineOf = {
            deep: { mode: "all", rows: [], complex: { text: '(genre:jazz | (composer:"thelonious monk" (label:"blue note" | label:riverside)))' } },
            engine: { mode: "all", complex: null, rows: [{ id: "0", field: "composer", kind: "list", negated: false, values: ["A Smith"] }, { id: "1", field: "genre", kind: "list", negated: false, values: ["Rock", "Drum and Bass"] }] },
            zero: { mode: "all", complex: null, rows: [{ id: "0", field: "genre", kind: "list", negated: false, values: ["Ambient"] }, { id: "1", field: "artist", kind: "list", negated: false, values: ["Iron Maiden"] }] } }
        QueryChips.notesOf = { engine: [{ from: "A. Smith", field: "composer", to: "A Smith", kind: "name" }, { from: "dnb", field: "genre", to: "Drum and Bass", kind: "alias" },
                                        { from: "rock", field: "genre", to: "Rock & Roll", kind: "near" }, { from: "rock", field: "genre", to: "Rock and Roll", kind: "near" }] }
        QueryChips.withoutOf = { zero: [{ id: "1", count: 412 }, { id: "0", count: 88 }] }
        QueryChips.checks = { "genre:jazz year: rating:4..": { ok: false, at: 16, message: "a year or a rating is a number" } }
        QueryChips.refinedText = "edited"
        QueryChips.lastRefineEdit = ({})
        panel.consoleShown = false
    }
    function all(root, name) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === name) out.push(child); walk(child) } }
        walk(root)
        return out
    }

    function test_a_query_too_deep_stays_as_text_and_opens_the_console() {
        Library.queryText = "deep"
        tryVerify(() => findChild(panel, "complexText") && findChild(panel, "complexText").visible, 2000)
        verify(findChild(panel, "complexText").text.indexOf(":filter (genre:jazz") === 0)
        verify(findChild(panel, "refineConsole").visible, "the console opens on its own")
        compare(all(panel, "conditionLabel").length, 0)
    }

    function test_what_the_engine_read_can_be_undone() {
        Library.queryText = "engine"
        tryVerify(() => all(panel, "engineNote").length === 3, 2000)
        const near = all(panel, "engineNote")[2]
        verify(findChild(near, "") !== null)
        const texts = []
        const walk = (item) => { for (const child of item.children) { if (child.text !== undefined && typeof child.text === "string") texts.push(child.text); walk(child) } }
        walk(near)
        verify(texts.indexOf("Rock & Roll, Rock and Roll") >= 0, "a widened genre reads as one note")
        const remove = all(near, "").find(c => c.tip !== undefined)
        remove.clicked()
        compare(QueryChips.lastRefineEdit, { op: "literal", note: 2 })
    }

    function test_the_console_is_hidden_until_asked_and_marks_an_error() {
        Library.queryText = "engine"
        verify(!findChild(panel, "refineConsole").visible)
        mouseClick(findChild(panel, "refineConsoleToggle"))
        tryVerify(() => findChild(panel, "refineConsole").visible, 1000)
        mouseClick(findChild(panel, "consoleEdit"))
        const input = findChild(panel, "consoleInput")
        tryVerify(() => input.visible, 1000)
        input.text = ":filter genre:jazz year: rating:4.."
        tryVerify(() => findChild(panel, "consoleError").visible, 1000)
        compare(findChild(panel, "consoleError").text, "24: a year or a rating is a number")
        keyClick(Qt.Key_Return)
        compare(Library.queryText, "engine", "a query that does not read is not applied")
        input.text = ":filter genre:jazz"
        keyClick(Qt.Key_Return)
        compare(Library.queryText, "genre:jazz")
    }

    function test_no_tracks_offers_to_take_one_condition_away() {
        Library.queryText = "zero"
        tryVerify(() => all(zero, "zeroOffer").length === 2, 2000)
        tryVerify(() => all(zero, "zeroOffer")[1].y > all(zero, "zeroOffer")[0].y, 2000, "the offers are laid out one under the other")
        mouseClick(all(zero, "zeroOffer")[0])
        compare(QueryChips.lastRefineEdit, { op: "remove", id: "1" })
        compare(Library.queryText, "edited")
    }
}
