import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "../../qml/Patterns"
import "../../qml/Components"
import "../../qml/Theme"

// With ratings off the Library's filters offer no stars: no rating column
// among the panes, no rating field to add in Refine and no stars to pick.
// A rating column already chosen stays chosen, and the conditions shown are
// read again from the facade, which leaves a rating condition out.
TestCase {
    id: test
    name: "RatingsOptOutFilters"
    when: windowShown
    visible: true
    width: 1200
    height: 500

    LibraryPanes { id: panes; width: 1200; height: 200; shown: ["year", "rating", "artist", "album"] }
    PickRatings { id: pick; y: 220; width: 300 }
    QueryChipRow { id: chips; y: 320; width: 600; query: "genre:jazz rating:4.." }

    function init() { failOnWarning(/Cannot create delegate|Object or context destroyed during incubation|invalid context|TypeError/) }
    function cleanup() { Prefs.ratings = true }
    function columns() {
        const out = []
        for (const row of panes.children)
            for (const column of row.children)
                if (column.column !== undefined && column.column !== "") out.push(column)
        return out.sort((a, b) => a.x - b.x).map(column => column.column).join(",")
    }

    function test_the_library_shows_no_rating_column_and_keeps_it_chosen() {
        waitForRendering(panes)
        compare(columns(), "year,rating,artist,album")
        Prefs.ratings = false
        tryCompare(test, "panesShown", "year,artist,album")
        compare(panes.shown, ["year", "rating", "artist", "album"], "the chosen columns are not changed")
        Prefs.ratings = true
        tryCompare(test, "panesShown", "year,rating,artist,album")
    }
    property string panesShown: { panes.columns; return columns() }

    function test_refine_offers_no_rating_field_and_no_stars() {
        verify(RefineText.order.indexOf("rating") >= 0 && pick.visible)
        Prefs.ratings = false
        compare(RefineText.order.indexOf("rating"), -1, "no rating field to add")
        verify(RefineText.order.indexOf("genre") >= 0, "the other fields stay")
        verify(!pick.visible, "no stars to pick")
        Prefs.ratings = true
        verify(RefineText.order.indexOf("rating") >= 0 && pick.visible)
    }

    function test_the_conditions_are_read_again_when_ratings_turn_on_or_off() {
        const before = QueryChips.refined.length
        Prefs.ratings = false
        verify(QueryChips.refined.length > before, "the chips ask the facade again")
        const off = QueryChips.refined.length
        Prefs.ratings = true
        verify(QueryChips.refined.length > off)
    }
}
