import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// The Library's filter columns: year, genre, artist and album by default,
// the columns turned on taking their place in order,
// and the width shared by the ones shown.
TestCase {
    id: test
    name: "LibraryPanes"
    when: windowShown
    visible: true
    width: 1200
    height: 300

    LibraryPanes { id: panes; width: 1200; height: 300 }

    function shown() {
        const out = []
        for (let i = 0; i < panes.children.length; ++i) {
            const row = panes.children[i]
            for (let j = 0; j < row.children.length; ++j)
                if (row.children[j].column !== undefined && row.children[j].column !== "")
                    out.push(row.children[j])
        }
        return out.sort((a, b) => a.x - b.x)
    }

    function test_default_columns() {
        panes.shown = ["year", "genre", "artist", "album"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "year,genre,artist,album")
    }

    function test_label_and_composer_take_their_place_in_order() {
        panes.shown = ["composer", "label", "year", "artist", "album"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "year,label,composer,artist,album")
        const total = shown().reduce((sum, c) => sum + c.width, 0)
        verify(Math.abs(total - (panes.width - 6)) < 2, "the columns share the width: " + total)
    }

    // Rating stands before the artist and draws its entries as stars.
    function test_rating_comes_before_the_artist_and_draws_its_stars() {
        panes.shown = ["rating", "year", "artist", "album"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "year,rating,artist,album")
        const list = Array.from(shown()[1].children).find(c => c.contentItem !== undefined)
        list.model.append({ key: "4", label: "4 stars", meta: "2", selected: false, all: false })
        tryVerify(() => (findChild(list, "paneStars") || {}).value === 4, 1000, "four stars drawn")
        list.model.clear()
    }

    function test_every_column_can_be_turned_off() {
        panes.shown = []
        waitForRendering(panes)
        compare(shown().length, 0)
        panes.shown = ["composer"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "composer")
    }
}
