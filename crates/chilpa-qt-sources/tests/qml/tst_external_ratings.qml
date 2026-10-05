import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../../chilpa-qt/qml/Components"

TestCase {
    name: "ExternalRatings"
    when: windowShown
    visible: true
    width: 1000; height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: true }
    RatingButton { id: album; y: 300; kind: "album"; target: -9 }
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
