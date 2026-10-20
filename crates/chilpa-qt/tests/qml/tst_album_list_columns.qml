import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A list of records shows the columns asked for, in their order, and a
// switch in the gear's modal turns one on or off.
TestCase {
    name: "AlbumListColumns"
    when: windowShown
    visible: true
    width: 1200; height: 600

    AlbumGridModel { id: records }
    AlbumGrid {
        id: grid
        width: 1100; height: 400
        list: true
        model: records
        listColumns: ["year", "tracks", "duration", "rating", "source"]
    }
    AlbumColumnsModal {
        id: chooser
        shown: grid.listColumns
        onChosen: (columns) => grid.listColumns = columns
    }

    function initTestCase() {
        AlbumActions.factsOf = {1: {tracks: 12, duration_ms: 3725000, genre: "Heavy Metal"}}
        Ratings.albums = {1: 4}
        records.append({first: 0, span: 1, group: ""})
    }

    function init() { tryVerify(() => findChild(grid, "tracksCell") !== null) }
    function cleanup() {
        chooser.close()
        records.mixed = false
        AlbumActions.sourcesOf = {}
        grid.listColumns = ["year", "tracks", "duration", "rating", "source"]
        records.rev++
    }

    function test_the_cells_say_tracks_length_and_stars_and_genre_waits_to_be_asked() {
        tryCompare(findChild(grid, "tracksCell"), "text", "12")
        compare(findChild(grid, "durationCell").text, "01:02:05", "hh:mm:ss")
        compare(findChild(grid, "ratingCell").value, 4)
        compare(findChild(grid, "genreCell").visible, false, "genre is off until asked for")
        compare(findChild(grid, "sourceCell").visible, false, "no source column where the list holds only the library's records")
        const cells = ["yearCell", "tracksCell", "durationCell", "ratingCell"].map(name => findChild(grid, name))
        for (let at = 1; at < cells.length; ++at)
            verify(cells[at].x > cells[at - 1].x, "the columns stand in their order")
    }

    function test_a_switch_turns_a_column_on_and_off() {
        chooser.open()
        tryVerify(() => chooser.opened)
        waitForRendering(chooser.contentItem)
        const genre = findChild(chooser.contentItem, "column-genre")
        verify(genre)
        compare(genre.checked, false)
        mouseClick(genre)
        tryCompare(grid, "listColumns", ["year", "tracks", "duration", "genre", "rating", "source"])
        tryCompare(findChild(grid, "genreCell"), "visible", true)
        compare(findChild(grid, "genreCell").text, "Heavy Metal")
        mouseClick(findChild(chooser.contentItem, "column-tracks"))
        tryCompare(findChild(grid, "tracksCell"), "visible", false)
    }

    function test_a_mixed_list_shows_every_source_the_record_is_on() {
        AlbumActions.sourcesOf = {1: ["local:default", "one", "two", "three", "four"].map(key =>
            ({key: key, icon: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png?" + key, local: key === "local:default"}))}
        records.mixed = true
        records.rev++
        const cell = findChild(grid, "sourceCell")
        tryCompare(cell, "visible", true)
        tryVerify(() => cell.children.filter(c => c.sourceInfo !== undefined).length === 5, 1000,
            "every source, not four of them")
    }
}
