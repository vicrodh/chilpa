import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"

// A playlist of the library reads as a source's playlist does: a header
// with its cover and the source it lives in, the bar between rules with its
// order, a search and how much it holds, and its tracks.
TestCase {
    name: "PlaylistPage"
    when: windowShown
    visible: true
    width: 1100
    height: 800

    PlaylistScreen { id: screen; width: 1100; height: 800; playlistId: 7 }

    function test_it_is_headed_and_searched_as_a_source_s_playlist() {
        const tracks = findChild(screen, "playlistTracks")
        for (let i = 0; i < 5; i++)
            tracks.append({ trackId: i + 1, rowKind: 0, number: i + 1, title: "Song " + i, artist: "Band", album: "Record", sourceName: "", duration: "3:00", plays: 0,
                quality: "FLAC", tier: 2, available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        verify(findChild(screen, "playlistHeaderCover") !== null, "a cover where a source's playlist has its own")
        verify(findChild(screen, "playlistPlay").visible)
        const find = findChild(screen, "playlistTrackSearch")
        verify(find.visible, "a search above the rows, as on a source's playlist")
        find.text = "song 3"
        tryCompare(tracks, "key", "song 3", 2000, "what is typed reaches the rows once the typing stops")
        findChild(screen, "playlistPlay").clicked()
        compare(tracks.lastAction, "playAll")
        screen.playlistId = 8
        compare(find.text, "", "another playlist starts with nothing typed")
    }
}
