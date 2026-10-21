import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/query/qml/Patterns"
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
        list.model.append({ key: "4", label: "4 stars", meta: "2", selected: false, excluded: false, all: false })
        tryVerify(() => (findChild(list, "paneStars") || {}).value === 4, 1000, "four stars drawn")
        list.model.clear()
    }

    function test_row_actions_and_excluded_appearance() {
        panes.shown = ["genre"]
        waitForRendering(panes)
        const list = Array.from(shown()[0].children).find(c => c.contentItem !== undefined)
        list.model.append({ key: "", label: "All", meta: "9", selected: true, excluded: false, all: true })
        list.model.append({ key: "Pop", label: "Pop", meta: "3", selected: false, excluded: true, all: false })
        tryVerify(() => list.itemAtIndex(1) !== null)
        const row = list.itemAtIndex(1)
        const minus = findChild(row, "paneExclude")
        verify(minus.visible)
        compare(minus.width, 20)
        compare(minus.height, 20)
        compare(minus.radius, 4)
        verify(findChild(row, "paneLabel").font.strikeout)
        compare(findChild(row, "paneLabel").color, Theme.color.textSecondary)
        verify(!findChild(list.itemAtIndex(0), "paneExclude").visible)
        mouseClick(row, 10, 5)
        compare(JSON.stringify(list.model.choices), "[[1,0]]")
        mouseClick(minus)
        compare(JSON.stringify(list.model.exclusions), "[1]")
        compare(list.model.choices.length, 1, "the minus never sends inclusion")
        mouseClick(row, 10, 5, Qt.LeftButton, Qt.AltModifier)
        compare(JSON.stringify(list.model.choices), "[[1,0],[1,4]]")
        mouseClick(row, 10, 5, Qt.LeftButton, Qt.ControlModifier | Qt.ShiftModifier)
        compare(JSON.stringify(list.model.choices), "[[1,0],[1,4],[1,3]]")
        list.model.setProperty(1, "excluded", false)
        verify(!findChild(row, "paneLabel").font.strikeout)
        list.model.clear()
    }

    Component { id: crumbsComponent; LibraryCrumbs { width: 600; height: 30 } }
    function test_crumbs_keep_only_the_scope_control() {
        Library.queryText = "genre:Jazz -year:1980"
        const crumbs = createTemporaryObject(crumbsComponent, test)
        verify(crumbs.filtered)
        const line = crumbs.children[0]
        compare(line.children.length, 1, "column crumbs are gone")
        compare(line.children[0].children[1].text, "All music")
        mouseClick(line.children[0].children[1])
        compare(Library.queryText, "")
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
