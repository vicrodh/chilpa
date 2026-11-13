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
        const confirmation = findChild(actions, "playlistDeletionConfirmation")
        tryCompare(confirmation, "opened", true)
        verify(confirmation.message.indexOf("Living room") >= 0, "the question names the server")
        const accept = findChild(confirmation.contentItem, "confirmationAccept")
        actions.selected = sidebar.rows.find(row => row.playlistId === 13)
        accept.clicked()
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
    function test_late_folders_and_bulk_updates_keep_one_indent_per_level() {
        sidebar.setOrder("custom")
        const source = "fixture", parentKey = source + " folder:parent", childKey = source + " folder:child"
        const leaf = {id:-1,name:"A retained",source:source,provider:"Catalog",native_folder:"folder:child",track_count:1}
        let catalog = {group_sources:true,folder_sources:[{id:source,name:"Catalog",icon:""}],native_folders:[],playlists:[leaf]}
        sidebar.expanded = ({[parentKey]:true,[childKey]:true})
        Playlists.catalog = catalog
        sidebar.reload()
        const view = findChild(sidebar,"sidebarPlaylistList")
        function host(id) {
            for (let index = 0; index < view.count; index++) {
                const item = view.itemAtIndex(index)
                if (item && item.entry.id === id) return item
            }
            return null
        }
        const id = "playlist:fixture:-1"
        view.positionViewAtBeginning()
        tryVerify(() => host(id)?.item !== null && host(id)?.item !== undefined)
        const retained = host(id).item
        const groupTitle = findChild(host("source:fixture").item,"playlistGroupTitle")
        const title = findChild(retained,"playlistTitle")
        const base = groupTitle.mapToItem(view,0,0).x
        verify(groupTitle.font.pointSize > title.font.pointSize)
        function check(depth) {
            const at = sidebar.rows.findIndex(row => row.id === id)
            tryVerify(() => view.count > at && view.model.get(at).entry.id === id && view.model.get(at).entry.depth === depth)
            view.forceLayout()
            view.positionViewAtIndex(at,ListView.Beginning)
            view.forceLayout()
            tryVerify(() => host(id)?.item && host(id).entry.depth === depth,1000,"playlist at depth " + depth)
            compare(host(id).x,0,"ListView positions the host without changing its indentation")
            const current = findChild(host(id).item,"playlistTitle")
            fuzzyCompare(current.mapToItem(view,0,0).x,base + depth * Metrics.playlistIndent,0.1)
        }
        check(1)
        catalog.native_folders = [{source:source,ref:"folder:parent",name:"Parent",parent:""}]
        Playlists.catalog = Object.assign({},catalog)
        sidebar.reload()
        tryCompare(view,"count",3)
        check(1)
        catalog.native_folders.push({source:source,ref:"folder:child",name:"Child",parent:"folder:parent"})
        Playlists.catalog = Object.assign({},catalog)
        sidebar.reload()
        tryCompare(view,"count",4)
        check(3)
        const childTitle = findChild(host("folder:fixture folder:child").item,"playlistFolderTitle")
        fuzzyCompare(childTitle.mapToItem(view,0,0).x,base + 2 * Metrics.playlistIndent,0.1)
        catalog.playlists = [leaf].concat(Array.from({length:40},(_,at)=>({id:-at-100,name:"Z list " + at,source:source,track_count:0})))
        Playlists.catalog = Object.assign({},catalog)
        sidebar.reload()
        tryCompare(view,"count",44)
        check(3)
        catalog.group_sources = false
        Playlists.catalog = Object.assign({},catalog)
        sidebar.reload()
        tryCompare(view,"count",43)
        check(2)
        catalog.playlists[0] = Object.assign({},leaf,{native_folder:""})
        Playlists.catalog = Object.assign({},catalog)
        sidebar.reload()
        compare(sidebar.rows.find(row => row.id === id).depth,0)
        check(0)
    }
}
