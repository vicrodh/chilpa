import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Components"

TestCase {
    name: "Ratings"
    when: windowShown
    visible: true
    width: 1000
    height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: true; reorderable: true }
    RatingStars {
        id: albumStars
        y: 300
        property int albumId: 1
        value: Ratings.album(albumId, Ratings.rev)
        onRated: (stars) => Ratings.rateAlbum(albumId, stars)
    }
    function init() {
        Ratings.tracks = ({}); Ratings.albums = ({}); Ratings.fail = false; ++Ratings.rev
        albumStars.albumId = 1
        list.rich = true
        tracks.clear()
        tracks.append({ trackId: 42, rowKind: 0, number: 1, title: "Track", artist: "Band",
            album: "Album", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
            playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        tracks.lastAction = ""
        wait(30)
    }
    function row() { return findChild(list, "trackListView").itemAtIndex(0) }
    function test_track_rating_never_selects_plays_or_reorders() {
        const item = row()
        verify(item !== null)
        const x = item.cols.ratingX + 3
        const y = item.height / 2
        mouseClick(item, x, y)
        compare(Ratings.track(42, 0), 1)
        compare(tracks.lastAction, "")
        mouseClick(item, x, y)
        compare(Ratings.track(42, 0), 0)
        mouseDoubleClickSequence(item, x, y)
        compare(tracks.lastAction, "")
        mousePress(item, x, y)
        mouseMove(item, x, y + 20)
        mouseRelease(item, x, y + 20)
        compare(tracks.lastAction, "")
    }
    function test_compact_rows_and_recycled_identity() {
        list.rich = false
        wait(10)
        const item = row()
        mouseClick(item, item.cols.ratingX + 3, item.height / 2)
        compare(item.rating, 1)
        tracks.setProperty(0, "trackId", 43)
        tryCompare(item, "rating", 0)
        Ratings.rateTrack(43, 5)
        tryCompare(item, "rating", 5)
        compare(Ratings.track(42, 0), 1)
    }
    function test_album_binding_survives_clicks_version_changes_and_errors() {
        mouseClick(albumStars, albumStars.width / 2, albumStars.height / 2)
        compare(albumStars.value, 3)
        albumStars.albumId = 2
        compare(albumStars.value, 0)
        Ratings.rateAlbum(2, 4)
        compare(albumStars.value, 4)
        Ratings.fail = true
        mouseClick(albumStars, 1, 1)
        compare(albumStars.value, 4)
        albumStars.albumId = 1
        compare(albumStars.value, 3)
    }
}
