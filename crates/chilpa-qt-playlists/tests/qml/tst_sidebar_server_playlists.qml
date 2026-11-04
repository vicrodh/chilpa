import QtQuick
import QtTest
import app.chilpa
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// A music server's own playlists in the sidebar: deleted there once asked
// again, renamed in place, and read again with the sidebar's Refresh.
TestCase {
    name: "SidebarServerPlaylists"
    when: windowShown
    visible: true
    width: 960
    height: 600
    SidebarPlaylists { id: sidebar; width: 240; height: 580 }
    SidebarPlaylistActions { id: actions; sidebar: sidebar }

    function init() {
        Playlists.catalog = {folders: [], playlists: [
            {id: 12, name: "Road Trip", source: "example:4f1c", provider: "Living room", mixed: false, server: true, writable: true,
                visible_sidebar: true, track_count: 4, artwork_path: "", covers: [], custom_position: 0, updated_at: 1, source_icon: "server"},
            {id: 13, name: "Shared", source: "example:4f1c", provider: "Living room", mixed: false, server: true, writable: false,
                visible_sidebar: true, track_count: 2, artwork_path: "", covers: [], custom_position: 1, updated_at: 2, source_icon: "server"},
            {id: 14, name: "Night", source: "example:4f1c", provider: "Living room", mixed: false, server: true, writable: true, unsent: true,
                visible_sidebar: true, track_count: 0, artwork_path: "", covers: [], custom_position: 2, updated_at: 3, source_icon: "server"}]}
        sidebar.reload()
    }
    function menu() { return findChild(actions, "playlistsMenu") }
    function offered() { return menu().entries.map(entry => entry.action) }

    function test_a_server_s_own_playlist_is_deleted_there_once_asked_again() {
        const road = sidebar.rows.find(row => row.playlistId === 12)
        verify(road.server && road.writable)
        actions.row(road, 0, 0)
        verify(offered().indexOf("deletePlaylist") >= 0)
        verify(offered().indexOf("edit") >= 0, "its name, and its description where the server takes one, are edited there")
        actions.row(sidebar.rows.find(row => row.playlistId === 13), 0, 0)
        verify(offered().indexOf("deletePlaylist") < 0, "one the account may only read is not deleted")
        actions.row(road, 0, 0)
        menu().chosen(offered().indexOf("deletePlaylist"))
        verify(menu().entries[0].text.indexOf("Living room") >= 0, "the question names the server: " + menu().entries[0].text)
        menu().chosen(offered().indexOf("confirmPlaylist"))
        compare(Playlists.removed, 12)
    }

    // The edit dialog of a server's own playlist leaves out what the server
    // does not keep: its picture and tags always, its description when the
    // server takes none.
    function test_a_server_s_playlist_is_edited_with_what_its_server_keeps() {
        const dialog = findChild(actions, "playlistEditDialog")
        const form = findChild(dialog, "playlistEditForm")
        PlaylistManager.serverPlaylists = {"12": {server: "Living room", icon: "server", writable: true, pending: false, describable: false}}
        actions.row(sidebar.rows.find(row => row.playlistId === 12), 0, 0)
        menu().chosen(offered().indexOf("edit"))
        verify(dialog.server && !form.localMetadata, "no picture or tags")
        verify(!form.described, "no description for a server that takes none")
        dialog.close()
        PlaylistManager.serverPlaylists = {"12": {server: "Living room", icon: "server", writable: true, pending: false, describable: true}}
        actions.row(sidebar.rows.find(row => row.playlistId === 12), 0, 0)
        menu().chosen(offered().indexOf("edit"))
        verify(form.described, "a description for one that takes it")
        dialog.close()
        PlaylistManager.serverPlaylists = ({})
    }

    // One made here and not sent yet wears the mark that says it waits.
    function test_a_playlist_not_sent_yet_is_marked_in_the_sidebar() {
        const night = sidebar.rows.find(row => row.playlistId === 14)
        verify(night.unsent, "the tree carries it")
        tryVerify(() => findChild(sidebar, "playlistUnsent") !== null)
        const marks = []
        const walk = item => { if (item.objectName === "playlistUnsent" && item.visible) marks.push(item); for (const child of item.children) walk(child) }
        walk(sidebar)
        compare(marks.length, 1, "only the one that waits")
    }

    function test_refresh_asks_the_servers_for_their_playlists_too() {
        const before = PlaylistManager.serverRefreshes
        actions.head(0, 0)
        menu().chosen(offered().indexOf("refresh"))
        compare(PlaylistManager.serverRefreshes, before + 1)
    }
}
