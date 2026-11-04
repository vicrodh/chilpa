import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Screens"
import "../../qml/Patterns"

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
    // One this page does not edit itself: the stub edits positive ids only.
    PlaylistScreen { id: serverPage; y: 900; width: 1100; height: 800; playlistId: -3 }
    PlaylistHeader { id: lone; y: 500; width: 900; covers: ["file:///covers/one.jpg"] }
    PlaylistHeader { id: several; y: 500; width: 900; covers: ["file:///covers/one.jpg", "file:///covers/two.jpg"] }

    // The covers come as addresses: one is drawn whole, two or more as a
    // mosaic of four, as the playlist's page always drew them.
    function test_the_cover_is_drawn_from_its_addresses() {
        const tiles = item => { const found = []; const walk = node => { if (node.hasOwnProperty("source") && node.hasOwnProperty("tint") && node.visible) found.push(node); for (const child of node.children || []) walk(child) }; walk(item); return found }
        const one = tiles(findChild(lone, "playlistHeaderCover"))
        compare(one.length, 1)
        compare(String(one[0].source), "file:///covers/one.jpg")
        const four = tiles(findChild(several, "playlistHeaderCover"))
        compare(four.length, 4)
        compare(String(four[1].source), "file:///covers/two.jpg")
    }

    // White over the cover's darkened field; on the dynamic background the
    // header reads in the theme's colors, as QBZ's playlist page.
    function test_on_the_dynamic_background_the_header_reads_in_the_theme_s_colors() {
        Playlists.coverRows = {[screen.playlistId]: ["file:///covers/one.jpg"]}
        SettingsModel.albumHeader = "full"
        const header = findChild(screen, "playlistHeaderCover").parent
        verify(screen.contrasted, "white over the cover's field")
        ThemeModel.ambient = true
        verify(!screen.contrasted)
        compare(header.ink, Theme.color.textSecondary)
        ThemeModel.ambient = false
        SettingsModel.albumHeader = "off"
        Playlists.coverRows = {}
    }

    function test_it_is_headed_and_searched_as_a_source_s_playlist() {
        const tracks = findChild(screen, "playlistTracks")
        for (let i = 0; i < 5; i++)
            tracks.append({ trackId: i + 1, rowKind: 0, number: i + 1, title: "Song " + i, artist: "Band", album: "Record", sourceName: "", duration: "3:00", plays: 0,
                quality: "FLAC", tier: 2, available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        verify(findChild(screen, "playlistHeaderCover") !== null, "a cover where a source's playlist has its own")
        verify(findChild(screen, "playlistHeaderCover").visible)
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

    // A music server's own playlist plays whole from its header, wears the
    // server's icon, and takes changes only while the account may make them
    // there and none is on its way.
    function test_a_server_s_playlist_plays_from_its_header_and_changes_through_its_server() {
        const play = findChild(serverPage, "playlistPlay")
        verify(!play.visible, "neither ours nor a server's: it does not play whole from here")
        PlaylistManager.serverPlaylists = {"-3": {server: "Living room", icon: "server", writable: true, pending: false, left_out: 2}}
        Playlists.rev++
        tryCompare(play, "visible", true)
        verify(serverPage.ofServer && serverPage.changeable, "the account may change it there")
        compare(serverPage.server.icon, "server")
        PlaylistManager.serverPlaylists = {"-3": {server: "Living room", icon: "server", writable: true, pending: true, left_out: 0}}
        Playlists.rev++
        tryCompare(serverPage, "changeable", false)
        PlaylistManager.serverPlaylists = {"-3": {server: "Living room", icon: "server", writable: false, pending: false, left_out: 0}}
        Playlists.rev++
        tryCompare(serverPage, "changeable", false)
        verify(play.visible, "only read here, it still plays")
        PlaylistManager.serverPlaylists = ({})
        Playlists.rev++
        tryCompare(play, "visible", false)
    }

    // While its server reads its library for the first time, a server's
    // playlist takes no change, and the page says why.
    function test_a_server_s_playlist_held_by_its_first_sync_says_so_and_takes_no_change() {
        const held = "Living room is still syncing its library for the first time. Try again when it finishes."
        PlaylistManager.serverPlaylists = {"-3": {server: "Living room", icon: "server", writable: true, pending: false, held: held, left_out: 0}}
        Playlists.rev++
        tryCompare(serverPage, "ofServer", true)
        compare(serverPage.changeable, false)
        compare(findChild(serverPage, "playlistHint").text, held)
        PlaylistManager.serverPlaylists = {"-3": {server: "Living room", icon: "server", writable: true, pending: false, held: "", left_out: 0}}
        Playlists.rev++
        tryCompare(serverPage, "changeable", true)
        PlaylistManager.serverPlaylists = ({})
        Playlists.rev++
    }
}
