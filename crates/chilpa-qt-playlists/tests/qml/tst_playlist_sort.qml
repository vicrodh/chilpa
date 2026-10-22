import QtQuick
import QtTest
import app.chilpa.playlists
import "../../qml/Components"

// Every playlist sorts the same way, as QBZ sorts them plus Most played,
// and each keeps the order chosen for it.
TestCase {
    name: "PlaylistSort"
    when: windowShown
    visible: true
    width: 600
    height: 400

    PlaylistSortButton { id: first; playlistId: 7 }
    PlaylistSortButton { id: other; y: 50; playlistId: -3 }
    PlaylistSortButton { id: fresh; y: 100; playlistId: 99 }

    function test_the_options_are_qbz_s_and_most_played() {
        compare(fresh.options.map(o => o.value), ["custom", "title", "artist", "album", "duration", "added", "plays"])
        compare(fresh.order, "custom", "its own order until one is chosen")
    }

    function test_a_choice_is_kept_for_that_playlist_alone() {
        first.picked("added")
        compare(first.order, "added")
        verify(first.ascending, "the last to come in first is its own way")
        compare(PlaylistManager.trackOrder(7), "added")
        compare(other.order, "custom", "another playlist keeps its own")
        first.picked("added-asc")
        verify(!first.ascending)
        first.playlistId = -3
        compare(first.order, "custom", "a playlist opened next shows the order it keeps")
        first.playlistId = 7
        compare(first.value, "added-asc")
    }
}
