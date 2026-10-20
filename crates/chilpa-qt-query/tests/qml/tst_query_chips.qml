import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Components"

// The conditions beyond the Library's columns, as chips: one per condition,
// the one kept out struck through, its words turn it around, its x takes it
// away, and "All of" or "Any of" swaps how they join.
TestCase {
    id: test
    name: "QueryChips"
    when: windowShown
    visible: true
    width: 900
    height: 100

    property string asked: ""
    QueryChipRow { id: row; query: "genre:Jazz -label:Verve"; onEdited: (query) => test.asked = query }

    function chips() {
        const out = []
        for (let i = 0; i < row.children.length; ++i)
            if (row.children[i].objectName === "queryChip") out.push(row.children[i])
        return out
    }

    function test_each_condition_is_a_chip_and_a_click_edits_it() {
        tryCompare(row, "visible", true)
        const all = chips()
        compare(all.length, 2)
        compare(findChild(all[0], "queryChipLabel").text, "Genre: Jazz")
        compare(findChild(all[0], "queryChipLabel").font.strikeout, false)
        compare(findChild(all[1], "queryChipLabel").font.strikeout, true, "a value kept out is struck through")
        mouseClick(findChild(all[1], "queryChipLabel"))
        compare(test.asked, "flipped:1")
        mouseClick(findChild(all[0], "queryChipRemove"))
        compare(test.asked, "without:0")
        const join = findChild(row, "queryJoin")
        verify(join.visible)
        compare(join.text, "All of")
        mouseClick(join)
        compare(test.asked, "joined:true")
    }

    function test_no_condition_shows_nothing() {
        row.query = ""
        tryCompare(row, "visible", false)
        row.query = "genre:Jazz -label:Verve"
    }
}
