import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "SidebarPlaylistOrganization"
    when: windowShown
    visible: true
    width: 960
    height: 600
    SidebarPlaylists { id: sidebar; width: 240; height: 580 }
    SidebarPlaylistActions { id: actions; sidebar: sidebar }
    function init() {
        Playlists.catalog = {folders: [
            {id: 1, name: "Empty", playlist_count: 0, color: "accent", position: 0},
            {id: 2, name: "Mixed", playlist_count: 2, color: "accent", position: 1}],
            playlists: [
                {id: -3, name: "Cloud", source: "com.test.cloud", provider: "Cloud", mixed: false,
                    folder_id: 2, separate: true, visible_sidebar: true, track_count: 4,
                    artwork_path: "", covers: [], custom_position: 0, updated_at: 1},
                {id: 7, name: "Local", source: "", provider: "Local", mixed: true,
                    folder_id: 2, separate: false, visible_sidebar: true, track_count: 2,
                    artwork_path: "", covers: [], custom_position: 1, updated_at: 2}]}
        sidebar.expanded = ({2: true})
        sidebar.setOrder("custom")
    }
    function test_empty_folders_and_mixed_children_share_the_tree() {
        compare(sidebar.rows.length, 4)
        compare(sidebar.rows[0].name, "Empty")
        compare(sidebar.rows[1].name, "Mixed")
        compare(sidebar.rows[2].playlistId, -3)
        verify(sidebar.rows[2].showSourceBadge)
        compare(sidebar.rows[3].playlistId, 7)
    }
    function test_folder_and_playlist_moves_preserve_foreign_identities() {
        actions.selected = sidebar.rows[1]
        actions.move(-1)
        compare(Playlists.created, [2, 1])
        actions.selected = sidebar.rows[2]
        actions.move(1)
        compare(Playlists.created, [7, -3])
    }
}
