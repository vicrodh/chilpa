import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"

TestCase {
    id: test
    name: "SidebarPlaylistOrganization"
    when: windowShown
    visible: true
    width: 960
    height: 600
    SidebarPlaylists { id: sidebar; width: 240; height: 580 }
    SidebarPlaylistActions { id: actions; sidebar: sidebar }
    function init() {
        Playlists.refreshes = 0
        Playlists.catalog = {folders: [
            {id: 1, name: "Empty", playlist_count: 0, color: "accent", position: 0},
            {id: 2, name: "Mixed", playlist_count: 2, color: "accent", position: 1}],
            playlists: [
                {id: -3, name: "Cloud", source: "com.test.cloud", provider: "Cloud", mixed: false,
                    folder_id: 2, separate: false, visible_sidebar: true, track_count: 4,
                    artwork_path: "", covers: [], custom_position: 0, updated_at: 1},
                {id: 7, name: "Local", source: "", provider: "Local", mixed: true,
                    folder_id: 2, separate: false, visible_sidebar: true, track_count: 2,
                    artwork_path: "", covers: [], custom_position: 1, updated_at: 2}]}
        sidebar.expanded = ({2: true})
        sidebar.setOrder("custom")
    }
    function longCatalog() {
        Playlists.catalog = {folders: [], playlists: Array.from({length: 40}, (_, index) => ({
            id: 100 + index, name: "List " + String(index).padStart(2, "0"), source: "", provider: "Local", mixed: true,
            folder_id: 0, visible_sidebar: true, track_count: index, artwork_path: "", covers: [], custom_position: index
        }))}
        sidebar.reload()
        const view = findChild(sidebar, "sidebarPlaylistList")
        tryCompare(view, "count", 40)
        view.positionViewAtIndex(12, ListView.Beginning)
        tryVerify(() => !!view.itemAtIndex(12)?.item)
        return view
    }
    function editing(view) {
        const host = view.itemAtIndex(12), row = host.item
        row.editing = true
        const input = test.Window.window.activeFocusItem
        verify(input && input.text !== undefined)
        input.text = "Uncommitted name"
        verify(row.editing)
        return {host: host, row: row, input: input, y: view.contentY}
    }
    function unchanged(view, kept) {
        compare(view.itemAtIndex(12), kept.host, "I retain the mounted delegate")
        compare(kept.host.item, kept.row, "I retain the playlist body")
        verify(kept.row.editing)
        compare(kept.input.text, "Uncommitted name")
        compare(test.Window.window.activeFocusItem, kept.input)
        fuzzyCompare(view.contentY, kept.y, 0.1)
    }
    function test_identical_remote_status_updates_keep_rename_focus_and_scroll() {
        const view = longCatalog(), kept = editing(view)
        for (let index = 0; index < 5; index++) {
            Library.remoteRev++
            Library.rev++
            Playlists.rev++
            wait(20)
            unchanged(view, kept)
        }
    }
    function test_another_playlist_update_patches_its_mounted_row() {
        const view = longCatalog(), kept = editing(view)
        tryVerify(() => !!view.itemAtIndex(13)?.item)
        const other = view.itemAtIndex(13), body = other.item
        Playlists.catalog = Object.assign({}, Playlists.catalog, {playlists: Playlists.catalog.playlists.map(row =>
            row.id === 113 ? Object.assign({}, row, {track_count: 99}) : row)})
        Library.remoteRev++
        tryCompare(body, "tracks", 99)
        compare(view.itemAtIndex(13), other)
        compare(other.item, body)
        unchanged(view, kept)
    }
    function test_custom_order_moves_existing_playlist_bodies_by_identity() {
        const view = longCatalog(), kept = editing(view)
        Playlists.catalog = Object.assign({}, Playlists.catalog, {playlists: Playlists.catalog.playlists.map(row =>
            Object.assign({}, row, {custom_position: row.id === 112 ? 13 : row.id === 113 ? 12 : row.custom_position}))})
        Library.remoteRev++
        tryVerify(() => view.itemAtIndex(13) === kept.host)
        compare(kept.host.item, kept.row)
        compare(test.Window.window.activeFocusItem, kept.input)
        compare(kept.input.text, "Uncommitted name")
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
    function test_refresh_action_rebuilds_the_sidebar_from_the_current_catalog() {
        const catalog=Object.assign({},Playlists.catalog,{playlists:Playlists.catalog.playlists.filter(p=>p.mixed)})
        Playlists.catalog=catalog
        actions.head(0,0)
        const menu=findChild(actions,"playlistsMenu")
        menu.chosen(menu.entries.findIndex(entry=>entry.action === "refresh"))
        compare(Playlists.refreshes,1)
        compare(sidebar.rows.filter(row=>row.rowKind === 0).map(row=>row.playlistId),[7])
    }
    function test_the_heads_actions_carry_their_glyphs_and_its_checks_none() {
        sidebar.setOrder("name")
        actions.head(0, 0)
        const menu = findChild(actions, "playlistsMenu")
        // An order or the grouping toggle shows its check; an action, its glyph.
        compare(menu.entries.filter(e => !e.separator && !e.accent && ("checked" in e) === !!e.glyph).map(e => e.text), [])
        compare(menu.entries.filter(e => e.accent).map(e => e.glyph), [Icons.chevronUp], "the order in use, its own way")
        menu.chosen(menu.entries.findIndex(e => e.accent))
        verify(!sidebar.ascending, "chosen again, it turns around")
        actions.head(0, 0)
        compare(menu.entries.filter(e => e.accent).map(e => e.glyph), [Icons.chevronDown])
        menu.chosen(menu.entries.findIndex(e => e.value === "recent"))
        verify(sidebar.ascending, "another order comes its own way")
    }
}
