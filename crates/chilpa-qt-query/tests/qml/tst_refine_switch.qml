import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Components"

// Columns | Refine: one shows, the same again hides it, and while Refine is
// not showing its segment counts the query's conditions (F1); narrow, the
// segments keep only their glyphs (F7).
TestCase {
    id: test
    name: "RefineSwitch"
    when: windowShown
    visible: true
    width: 600
    height: 100

    RefineSwitch { id: toggle }

    function init() {
        QueryChips.refineOf = { q: { mode: "all", complex: null, rows: [
            { id: "0", field: "genre", kind: "list", negated: false, values: ["Jazz"] },
            { id: "1", group: true, mode: "any", rows: [
                { id: "1.0", field: "genre", kind: "list", negated: false, values: ["Soul"] },
                { id: "1.1", field: "composer", kind: "list", negated: false, values: ["Monk"] } ] } ] } }
        Library.queryText = "q"
        SettingsModel.panesOpen = false
        toggle.narrow = false
    }
    function badge() {
        const segments = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === "segmentBadge") segments.push(child); walk(child) } }
        walk(toggle)
        return segments[1]
    }

    function test_the_badge_counts_conditions_while_refine_is_out_of_sight() {
        compare(toggle.conditions, 3, "a group counts its own conditions")
        verify(badge().visible)
        mouseClick(findChild(toggle, "segment"), 5, 5)
        verify(SettingsModel.panesOpen && !toggle.refineMode, "the columns show")
        verify(badge().visible, "with the columns, the query still counts")
        toggle.show(true)
        verify(toggle.refineShown)
        verify(!badge().visible, "Refine itself shows the conditions")
    }

    function test_choosing_the_shown_one_hides_the_area() {
        toggle.show(true)
        toggle.activated(1)
        verify(!SettingsModel.panesOpen)
        toggle.activated(1)
        verify(SettingsModel.panesOpen && toggle.refineMode)
    }

    function test_narrow_keeps_only_the_glyphs() {
        const before = toggle.width
        toggle.narrow = true
        tryVerify(() => toggle.width < before, 1000, "the words go, the glyphs stay")
    }
}
