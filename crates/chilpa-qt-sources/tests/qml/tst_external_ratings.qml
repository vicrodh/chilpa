import QtQuick
import QtTest
import app.chilpa
import "../../../chilpa-qt/qml/Components"

TestCase {
    name: "ExternalRatings"
    when: windowShown
    visible: true
    width: 1000; height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: true }
    RatingButton { id: album; y: 300; kind: "album"; target: -9 }
    function init() { album.target = -9 }
    function test_album_flyout_rates_reopens_and_closes_on_identity_change() {
        Ratings.tracks = ({}); Ratings.albums = ({}); Ratings.fail = false; ++Ratings.rev
        album.target = -9
        const popup = findChild(album, "ratingFlyout")
        verify(popup !== null)
        mouseClick(album, album.width / 2, album.height / 2)
        tryCompare(popup, "opened", true)
        const stars = findChild(popup.contentItem, "ratingFlyoutStars")
        verify(stars !== null)
        mouseClick(stars, stars.size * 4.5, stars.height / 2)
        compare(album.value, 5)
        keyClick(Qt.Key_Escape)
        tryCompare(popup, "visible", false)
        mouseClick(album, album.width / 2, album.height / 2)
        tryCompare(popup, "opened", true)
        album.target = -10
        tryCompare(popup, "visible", false)
        compare(album.value, 0)
        compare(Ratings.stars("album", -9, 0), 5)
    }
    function test_disconnected_source_takes_personal_stars_without_playback() {
        Ratings.tracks = ({}); Ratings.albums = ({}); Ratings.fail = false; ++Ratings.rev
        tracks.clear()
        tracks.append({ trackId: -42, rowKind: 0, number: 1, title: "Track", artist: "Band",
            album: "Album", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
            available: false, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        tracks.lastAction = ""
        wait(30)
        const row = findChild(list, "trackListView").itemAtIndex(0)
        mouseClick(row, row.cols.ratingX + 3, row.height / 2)
        compare(Ratings.stars("track", -42, 0), 1)
        compare(tracks.lastAction, "")
        verify(album.visible)
        Ratings.rate("album", -9, 4)
        compare(album.value, 4)
        album.target = Number("Artist name")
        verify(!album.visible)
    }
}
