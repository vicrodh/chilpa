import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components"

// With ratings off the window's own pieces draw no stars: a track list has
// no rating column and its rows take no rating, a page keeps its heart
// without the star button, and a list of records keeps its rating column
// chosen without showing it. Turning ratings on or off reads the Library
// again, which filters by them or not.
TestCase {
    id: test
    name: "RatingsOptOutWindow"
    when: windowShown
    visible: true
    width: 1100; height: 700

    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 200; model: tracks; rich: true }
    MarkButtons { id: marks; y: 220; kind: "album"; key: "1" }
    RatingButton { id: alone; x: 200; y: 220; kind: "playlist"; target: 3 }
    AlbumGridModel { id: records }
    AlbumGrid { id: grid; y: 280; width: 1000; height: 300; list: true; model: records; listColumns: ["year", "rating"] }
    WindowRequests {}

    function initTestCase() {
        Ratings.albums = {1: 4}
        records.append({first: 0, span: 1, group: "", revision: 0})
    }
    function init() {
        Ratings.tracks = ({}); Ratings.fail = false; ++Ratings.rev
        tracks.clear()
        tracks.append({ trackId: 42, rowKind: 0, number: 1, title: "Track", artist: "Band",
            album: "Album", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
            available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        wait(30)
    }
    function cleanup() { Prefs.ratings = true }
    function row() { return findChild(list, "trackListView").itemAtIndex(0) }
    function header() {
        const walk = (item) => {
            if (item.titles !== undefined && item.cols !== undefined) return item
            for (const child of item.children) { const found = walk(child); if (found) return found }
            return null
        }
        return walk(list)
    }
    function ratingTitle() { return header().titles.find(title => title.text === list.columns.names.rating) }

    function test_a_track_list_has_no_rating_column_and_takes_no_rating() {
        Ratings.tracks = {43: 3}; ++Ratings.rev
        const item = row()
        verify(item.cols.rating > 0 && ratingTitle().w > 0)
        const x = item.cols.ratingX + 3, y = item.height / 2
        Prefs.ratings = false
        compare(item.cols.rating, 0, "no rating column")
        compare(ratingTitle().w, 0, "no rating title")
        mouseClick(item, x, y)
        compare(Ratings.stars("track", 42, 0), 0, "a click where the stars stood rates nothing")
        tracks.setProperty(0, "trackId", 43)
        compare(item.rating, 0, "a row reads no stars")
        Prefs.ratings = true
        tryCompare(item, "rating", 3, 1000, "on again, the stars given are back")
        tracks.setProperty(0, "trackId", 42)
        mouseClick(item, item.cols.ratingX + 3, y)
        compare(Ratings.stars("track", 42, 0), 1, "on again, the stars rate")
    }

    function test_a_page_keeps_its_heart_and_loses_the_star_button() {
        const children = Array.from(marks.children)
        const heart = children.find(child => child.heart === true)
        const star = children.find(child => child.kind !== undefined && child.target !== undefined)
        verify(heart.visible && star.visible && alone.visible)
        Prefs.ratings = false
        verify(!star.visible, "no star button beside the heart")
        verify(!alone.visible, "no star button anywhere")
        verify(heart.visible, "the heart stays")
        Prefs.ratings = true
        verify(star.visible && alone.visible)
    }

    function test_a_list_of_records_hides_its_rating_cell_and_keeps_the_column() {
        tryVerify(() => findChild(grid, "ratingCell") !== null)
        const cell = findChild(grid, "ratingCell")
        tryCompare(cell, "visible", true)
        compare(cell.value, 4)
        Prefs.ratings = false
        tryCompare(cell, "visible", false)
        compare(grid.listColumns, ["year", "rating"], "the chosen columns are not changed")
        Prefs.ratings = true
        tryCompare(cell, "visible", true)
        compare(cell.value, 4, "the stars given are back")
    }

    function test_turning_ratings_off_or_on_reads_the_library_again() {
        Library.queryText = "genre:jazz rating:4.."
        const before = Library.applied
        Prefs.ratings = false
        compare(Library.applied, before + 1)
        compare(Library.queryText, "genre:jazz rating:4..", "the query keeps its rating condition")
        Prefs.ratings = true
        compare(Library.applied, before + 2)
        Library.queryText = ""
    }
}
