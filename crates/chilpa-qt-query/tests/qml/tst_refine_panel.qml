import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Patterns"
import "../../qml/Components"

// Refine (F2, F4, F5): empty it says so in one line; each condition is a
// row whose operator, values and bin edit the query through the facade; two
// or more conditions bring "Match all | any"; a group is a box with its own
// switch; "Group" opens an empty box new conditions go into.
TestCase {
    id: test
    name: "RefinePanel"
    when: windowShown
    visible: true
    width: 1100
    height: 700

    readonly property var full: ({ mode: "all", complex: null, rows: [
        { id: "0", field: "genre", kind: "list", negated: false, values: ["Jazz", "Soul"] },
        { id: "1", field: "genre", kind: "list", negated: true, values: ["Smooth Jazz"] },
        { id: "2", field: "year", kind: "range", from: 1960, to: 1969 },
        { id: "3", field: "rating", kind: "rating", min: 4, max: 5 },
        { id: "4", field: "added", kind: "added", n: 6, unit: "w" },
        { id: "5", field: "label", kind: "none" },
        { id: "6", field: "title", kind: "words", values: ["blue"] } ] })
    readonly property var grouped: ({ mode: "all", complex: null, rows: [
        { id: "0", group: true, mode: "any", rows: [
            { id: "0.0", field: "genre", kind: "list", negated: false, values: ["Jazz"] },
            { id: "0.1", field: "composer", kind: "list", negated: false, values: ["Thelonious Monk"] } ] },
        { id: "1", field: "year", kind: "range", from: 1960, to: 1969 } ] })

    RefinePanel { id: panel; width: 1000; height: 600; summary: "256 songs · 18 h" }

    function init() {
        QueryChips.refineOf = { full: test.full, grouped: test.grouped }
        QueryChips.refinedText = "edited"
        QueryChips.lastRefineEdit = ({})
        panel.target = ""
        panel.picking = ""
        Library.queryText = "full"
    }
    function all(name) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === name) out.push(child); walk(child) } }
        walk(panel)
        return out
    }
    function rows() { return all("conditionLabel").map(label => label.parent) }

    function test_empty_says_so_and_offers_every_field() {
        Library.queryText = ""
        tryVerify(() => findChild(panel, "refineEmpty").visible)
        verify(!findChild(panel, "refineMatch").visible)
        compare(all("fieldChip").length, 12)
        verify(!findChild(panel, "groupChip").visible, "nothing to group yet")
    }

    function test_each_condition_is_a_row() {
        tryVerify(() => rows().length === 7, 2000)
        const operators = all("operatorPill").map(pill => pill.children[0].children[1].text)
        compare(operators, ["is any of", "is not", "between", "at least", "in the last", "has none", "contains"])
        verify(findChild(panel, "refineMatch").visible, "two or more bring Match")
        verify(findChild(panel, "groupChip").visible)
    }

    function test_rows_edit_through_the_facade() {
        tryVerify(() => rows().length === 7, 2000)
        const pill = all("operatorPill")[0]
        mouseClick(pill)
        const choices = all("operatorChoices")[0]
        tryVerify(() => choices.visible)
        choices.activated(1)
        compare(QueryChips.lastRefineEdit, { op: "operator", id: "0", to: "exclude" })
        compare(Library.queryText, "edited")
        Library.queryText = "full"
        tryVerify(() => rows().length === 7, 2000)
        mouseClick(all("valueRemove")[1])
        compare(QueryChips.lastRefineEdit, { op: "removeValue", id: "0", value: "Soul" })
        Library.queryText = "full"
        tryVerify(() => rows().length === 7, 2000)
        mouseClick(all("conditionRemove")[3])
        compare(QueryChips.lastRefineEdit, { op: "remove", id: "3" })
    }

    function test_match_any_and_clear_all() {
        tryVerify(() => findChild(panel, "refineMatch").visible)
        findChild(panel, "refineMatch").activated(1)
        compare(QueryChips.lastRefineEdit, { op: "mode", mode: "any" })
        Library.queryText = "full"
        mouseClick(findChild(panel, "refineClear"))
        compare(Library.queryText, "")
    }

    function test_a_group_is_a_box_with_its_own_switch() {
        Library.queryText = "grouped"
        tryVerify(() => all("groupMatch").length === 1, 2000)
        compare(rows().length, 3, "the group's two rows and the year")
        all("groupMatch")[0].activated(0)
        compare(QueryChips.lastRefineEdit, { op: "mode", mode: "all", target: "0" })
        Library.queryText = "grouped"
        tryVerify(() => all("groupUngroup").length === 1, 2000)
        mouseClick(all("groupUngroup")[0])
        compare(QueryChips.lastRefineEdit, { op: "ungroup", id: "0" })
    }

    function test_group_opens_an_empty_box_new_conditions_go_into() {
        tryVerify(() => findChild(panel, "groupChip").visible)
        mouseClick(findChild(panel, "groupChip"))
        compare(panel.target, "new")
        tryVerify(() => all("groupMatch").length === 1, 2000)
        panel.edit({ op: "value", field: "genre", value: "Jazz", state: "include" })
        compare(QueryChips.lastRefineEdit.target, "new", "the first condition makes the group")
        compare(panel.target, "")
    }

    function test_undo_and_the_count() {
        QueryChips.canUndo = true
        QueryChips.undoneText = "before"
        mouseClick(findChild(panel, "refineUndo"))
        compare(Library.queryText, "before")
        QueryChips.canUndo = false
        const count = findChild(panel, "refineCount")
        compare(count.children[0].text, "256")
    }
}
