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
        QueryChips.answers = true
        QueryChips.lastTyped = ""
        QueryChips.closed = []
        QueryChips.suggestCalls = 0
        findChild(panel, "refineAddInput").text = ""
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

    // The counts arrive after the typing moved on: an answer for the older
    // text is not shown, and the list keeps what it had until its own comes.
    function test_an_answer_for_older_text_does_not_replace_the_newer() {
        QueryChips.answers = false
        const input = findChild(panel, "refineAddInput")
        input.forceActiveFocus()
        input.text = "mon"
        tryVerify(() => QueryChips.lastTyped === "mon", 2000)
        compare(all("suggestion").length, 0, "nothing until an answer comes")
        input.text = "monk"
        tryVerify(() => QueryChips.lastTyped === "monk", 2000)
        const consumer = QueryChips.lastConsumer
        QueryChips.suggested(consumer, "jazz", "mon", JSON.stringify({ groups: [{ field: "artist", items: [{ value: "Monday", count: 1, state: "none" }] }], words: {} }))
        wait(50)
        compare(all("suggestion").length, 0, "the answer for 'mon' is not what is typed")
        QueryChips.suggested(consumer, "jazz", "monk", JSON.stringify(QueryChips.suggestions))
        tryVerify(() => all("suggestion").length === 2, 2000)
        QueryChips.suggested(consumer, "jazz", "mon", JSON.stringify({ groups: [], words: {} }))
        wait(50)
        compare(all("suggestion").length, 2, "a late older answer does not take it back")
        QueryChips.suggested(consumer + 1, "jazz", "monk", JSON.stringify({ groups: [], words: {} }))
        wait(50)
        compare(all("suggestion").length, 2, "another list's answer is not this one's")
    }

    // Enter while the answer for what is typed now is on its way applies
    // that answer when it comes, never the one shown for the older text; an
    // empty answer falls back to the words.
    function test_enter_waits_for_the_answer_to_what_is_typed_now() {
        QueryChips.answers = false
        const input = findChild(panel, "refineAddInput")
        input.forceActiveFocus()
        input.text = "monk"
        tryVerify(() => QueryChips.lastTyped === "monk", 2000)
        QueryChips.suggested(QueryChips.lastConsumer, "jazz", "monk", JSON.stringify(QueryChips.suggestions))
        tryVerify(() => all("suggestion").length === 2, 2000)
        input.text = "bach"
        QueryChips.lastRefineEdit = ({})
        keyClick(Qt.Key_Return)
        tryVerify(() => QueryChips.lastTyped === "bach", 2000)
        compare(QueryChips.lastRefineEdit, {}, "nothing applied: Monk was the answer for other text")
        QueryChips.suggested(QueryChips.lastConsumer, "jazz", "bach", JSON.stringify({ groups: [{ field: "composer", items: [{ value: "J. S. Bach", count: 3, state: "none" }] }], words: {} }))
        tryVerify(() => QueryChips.lastRefineEdit.value === "J. S. Bach", 2000)
        compare(QueryChips.lastRefineEdit.field, "composer")
        Library.queryText = "jazz"
        input.text = "zzz"
        tryVerify(() => QueryChips.lastTyped === "zzz", 2000)
        keyClick(Qt.Key_Return)
        QueryChips.suggested(QueryChips.lastConsumer, "jazz", "zzz", JSON.stringify({ groups: [], words: {} }))
        tryVerify(() => QueryChips.lastRefineEdit.op === "words" && QueryChips.lastRefineEdit.word === "zzz", 2000)
    }

    // Behind the panel, so a list left over by a failed test takes no click.
    Component { id: lone; SuggestionList { width: 600; z: -1 } }
    // A condition changed under the list: its counts go, and it asks again
    // for the same text under the new query.
    function test_a_query_change_clears_the_counts_and_asks_again() {
        QueryChips.answers = false
        const list = lone.createObject(test, { typed: "monk" })
        tryVerify(() => QueryChips.lastTyped === "monk" && QueryChips.lastSuggestQuery === "jazz", 2000)
        const consumer = QueryChips.lastConsumer
        QueryChips.suggested(consumer, "jazz", "monk", JSON.stringify(QueryChips.suggestions))
        tryVerify(() => list.found.groups.length === 2, 2000)
        Library.queryText = "rock"
        compare(list.found.groups.length, 0, "the counts were for jazz")
        tryVerify(() => QueryChips.lastSuggestQuery === "rock" && QueryChips.lastTyped === "monk", 2000)
        QueryChips.suggested(consumer, "rock", "monk", JSON.stringify(QueryChips.suggestions))
        tryVerify(() => list.found.groups.length === 2, 2000)
        list.destroy()
        tryVerify(() => QueryChips.closed.indexOf(consumer) >= 0, 2000)
    }

    // Enter pressed again, or held, while the round for the text is on its
    // way asks no second round: only the intent changes, and the answer
    // applies the last one.
    function test_enter_pressed_again_keeps_the_round_and_changes_only_the_intent() {
        QueryChips.answers = false
        const input = findChild(panel, "refineAddInput")
        input.forceActiveFocus()
        input.text = "monk"
        tryVerify(() => QueryChips.lastTyped === "monk", 2000)
        const calls = QueryChips.suggestCalls
        keyClick(Qt.Key_Return)
        keyClick(Qt.Key_Return)
        keyClick(Qt.Key_Return, Qt.ShiftModifier)
        compare(QueryChips.suggestCalls, calls, "the round asked for 'monk' is kept")
        QueryChips.suggested(QueryChips.lastConsumer, "jazz", "monk", JSON.stringify(QueryChips.suggestions))
        tryVerify(() => QueryChips.lastRefineEdit.value === "Thelonious Monk", 2000)
        compare(QueryChips.lastRefineEdit.state, "exclude", "the last press was Shift+Enter")
    }

    // Hidden, the list hands its handle back so the facade can let its
    // thread go; shown again it takes a new one, and asks only when what it
    // shows is not for the text and query of now.
    function test_a_hidden_list_lets_its_handle_go_and_asks_again_when_shown() {
        QueryChips.answers = false
        const list = lone.createObject(test, { typed: "monk" })
        tryVerify(() => QueryChips.lastTyped === "monk", 2000)
        const first = QueryChips.lastConsumer
        compare(list.consumer, first)
        QueryChips.suggested(first, "jazz", "monk", JSON.stringify(QueryChips.suggestions))
        tryVerify(() => list.found.groups.length === 2, 2000)
        list.visible = false
        compare(list.consumer, 0, "no handle while hidden")
        verify(QueryChips.closed.indexOf(first) >= 0, "and the bridge was told")
        const calls = QueryChips.suggestCalls
        list.visible = true
        verify(list.consumer !== 0 && list.consumer !== first, "a new handle on return")
        wait(250)
        compare(QueryChips.suggestCalls, calls, "what it shows is still for jazz and monk: no round")
        compare(list.found.groups.length, 2, "and it is still shown")
        list.visible = false
        Library.queryText = "rock"
        wait(250)
        compare(QueryChips.suggestCalls, calls, "hidden, a query change asks nothing")
        list.visible = true
        tryVerify(() => QueryChips.suggestCalls === calls + 1 && QueryChips.lastSuggestQuery === "rock", 2000)
        list.destroy()
    }

    Component { id: windowed; Window { width: 400; height: 300; property alias list: inner; SuggestionList { id: inner; width: 380; typed: "monk" } } }
    // Refine in its own window, closed from the window's own button: the
    // window hides, the list stays, and its handle goes just the same.
    function test_a_list_in_a_window_closed_lets_its_handle_go() {
        QueryChips.answers = false
        const window = windowed.createObject(test)
        window.visible = true
        tryVerify(() => window.list.consumer !== 0, 2000)
        const first = window.list.consumer
        window.visible = false
        tryVerify(() => window.list.consumer === 0, 2000)
        verify(QueryChips.closed.indexOf(first) >= 0)
        window.visible = true
        tryVerify(() => window.list.consumer !== 0 && window.list.consumer !== first, 2000)
        window.destroy()
    }

    // The search above a field's values narrows them; it once reached
    // PickValues as undefined, its own property shadowing the field's id.
    function test_the_search_narrows_a_fields_values() {
        failOnWarning(/.*/)
        panel.picking = "genre"
        tryVerify(() => findValue("Jazz") !== null, 2000)
        const search = findChild(panel, "pickSearch")
        search.forceActiveFocus()
        search.text = "so"
        tryVerify(() => findValue("Soul") !== null && findValue("Jazz") === null, 2000)
    }
}
