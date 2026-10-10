import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// The Library's filter columns: year and genre by default with artist and
// album always there, the columns turned on taking their place in order,
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

    function test_year_and_genre_by_default_and_artist_and_album_always() {
        panes.shown = ["year", "genre"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "year,genre,artist,album")
    }

    function test_label_and_composer_take_their_place_in_order() {
        panes.shown = ["composer", "label", "year"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "year,label,composer,artist,album")
        const total = shown().reduce((sum, c) => sum + c.width, 0)
        verify(Math.abs(total - (panes.width - 6)) < 2, "the columns share the width: " + total)
    }

    // Rating stands before the artist; its entries are drawn as stars, and
    // the records with none are named.
    function test_rating_comes_before_the_artist_and_draws_its_stars() {
        panes.shown = ["rating", "year"]
        waitForRendering(panes)
        const rating = shown().find(c => c.column === "rating")
        compare(shown().map(c => c.column).join(","), "year,rating,artist,album")
        let list = null
        for (let i = 0; i < rating.children.length; ++i)
            if (rating.children[i].contentItem !== undefined) list = rating.children[i]
        list.model.append({ key: "4", label: "4 stars", meta: "2", selected: false, all: false })
        list.model.append({ key: "0", label: "Unrated", meta: "5", selected: false, all: false })
        tryVerify(() => findChild(list, "paneStars") !== null)
        compare(findChild(list, "paneStars").value, 4)
        list.model.clear()
    }

    function test_none_turned_on_leaves_artist_and_album() {
        panes.shown = []
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "artist,album")
    }
}
