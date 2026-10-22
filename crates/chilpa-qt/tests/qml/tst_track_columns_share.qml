import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../../qml/Components"

// The flexible columns share the room by what they hold: what one does not
// need goes to the one that would be cut, and with nothing said, the
// design's proportions.
TestCase {
    name: "TrackColumnsShare"
    TrackColumns { id: cols; width: 1300; showAlbum: true; showArtist: true }

    function cleanup() { cols.wants = ({}); cols.width = 1300 }

    function test_a_long_album_takes_the_room_short_titles_leave() {
        const proportional = cols.albumW
        cols.wants = {title: 220, artist: 110, album: 420}
        verify(cols.albumW >= 420 + Metrics.gap12, "the album is not cut: " + cols.albumW)
        verify(cols.albumW > proportional, "it took room the others did not need")
        verify(cols.titleW >= 220 + Metrics.gap12 && cols.artistW >= 110 + Metrics.gap12)
        compare(Math.round(cols.titleW + cols.artistW + cols.albumW), Math.round(cols.free), "the whole room is used")
    }

    function test_when_nothing_fits_each_keeps_its_share_and_no_more_than_it_wants() {
        cols.width = 700
        cols.wants = {title: 2000, artist: 60, album: 2000}
        compare(cols.artistW, Math.max(cols.least, 60 + Metrics.gap12), "a short column takes what it wants")
        const rest = cols.free - cols.artistW
        compare(Math.round(cols.titleW), Math.round(rest * cols.titleShare / (cols.titleShare + cols.albumShare)))
    }

    function test_without_wants_the_proportions_stand() {
        const unit = cols.free / (cols.titleShare + cols.artistShare + cols.albumShare)
        compare(Math.round(cols.titleW), Math.round(unit * cols.titleShare))
        compare(Math.round(cols.albumW), Math.round(unit * cols.albumShare))
    }

    TrackListModel { id: queue; measures: true }
    TrackList { id: list; y: 0; width: 1100; height: 300; model: queue; rich: false; visible: false }

    // A queue of short titles from one long record: the record's name is not
    // cut while the titles leave room (the owner's Cranberries queue).
    function test_the_rows_say_what_the_columns_want() {
        queue.clear()
        for (let i = 0; i < 12; ++i)
            queue.append({trackId: i + 1, rowKind: 0, number: i + 1, title: ["Dreams", "Linger", "Pretty"][i % 3], artist: "The Cranberries",
                album: "Everybody Else Is Doing It, So Why Can't We?", composer: "D. O'an, N. Hogan", sourceName: "", sourceInfo: "{}",
                duration: "3:00", plays: 0, quality: "FLAC", tier: 2, available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: ""})
        tryVerify(() => (list.columns.wants.album || 0) > 0)
        verify(list.columns.albumW >= list.columns.wants.album + Metrics.gap12, "the album fits: " + list.columns.albumW + " for " + list.columns.wants.album)
        verify(list.columns.titleW >= list.columns.wants.title + Metrics.gap12, "and the title with its credit")
    }

    function test_a_hidden_column_takes_nothing() {
        cols.showArtist = false
        cols.wants = {title: 100, artist: 900, album: 100}
        compare(cols.artistW, 0)
        cols.showArtist = true
    }
}
