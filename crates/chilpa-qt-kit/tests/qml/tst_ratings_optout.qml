import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// With ratings off the kit's pieces draw no stars: the stars themselves, a
// track list's rating column, a record's rating cell and the switch that
// chooses it. A rating column already chosen stays chosen, and everything
// is back when ratings are on again.
TestCase {
    id: test
    name: "RatingsOptOutKit"
    when: windowShown
    visible: true
    width: 900; height: 600

    property var chosen: []
    RatingStars { id: stars; value: 3 }
    TrackColumns { id: columns; width: 800 }
    AlbumListCells { id: cells; y: 40; height: 40; columns: ["year", "rating"]; year: "1999"; stars: 4 }
    AlbumColumnsGear { id: gear; y: 100; shown: ["year", "tracks", "rating"]; onChosen: (picked) => test.chosen = picked }

    function cleanup() { Prefs.ratings = true; test.chosen = [] }

    function test_the_stars_hide_and_come_back() {
        verify(stars.visible)
        Prefs.ratings = false
        verify(!stars.visible, "no stars while ratings are off")
        Prefs.ratings = true
        verify(stars.visible)
    }

    function test_a_track_list_gives_its_rating_column_to_the_others() {
        const rating = columns.rating, free = columns.free
        verify(rating > 0)
        Prefs.ratings = false
        compare(columns.rating, 0, "the rating column measures nothing")
        compare(columns.free, free + rating, "the flexible columns take its room")
        Prefs.ratings = true
        compare(columns.rating, rating)
    }

    function test_a_records_rating_cell_hides_while_its_column_stays_chosen() {
        const cell = findChild(cells, "ratingCell")
        verify(cell.visible)
        Prefs.ratings = false
        verify(!cell.visible)
        verify(!cells.shows("rating"))
        verify(cells.shows("year"), "the other columns stay")
        compare(cells.columns, ["year", "rating"], "the chosen columns are not changed")
        Prefs.ratings = true
        verify(cell.visible)
    }

    function test_the_column_chooser_offers_no_rating_and_keeps_it_chosen() {
        mouseClick(gear)
        const modal = findChild(gear, "albumColumns")
        tryVerify(() => modal.opened)
        waitForRendering(modal.contentItem)
        verify(findChild(modal.contentItem, "column-rating") !== null)
        Prefs.ratings = false
        tryVerify(() => findChild(modal.contentItem, "column-rating") === null, 1000, "no rating switch while ratings are off")
        waitForRendering(modal.contentItem)
        const tracks = findChild(modal.contentItem, "column-tracks")
        verify(tracks.checked)
        mouseClick(tracks)
        compare(chosen, ["year", "rating"], "turning another column off keeps the rating column chosen")
        Prefs.ratings = true
        tryVerify(() => findChild(modal.contentItem, "column-rating") !== null)
        modal.close()
    }
}
