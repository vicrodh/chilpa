import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "../../../chilpa-qt/qml/Components"
import "../../qml/Components"

TestCase {
    id: test
    name: "ServerOffline"
    when: windowShown
    visible: true
    width: 1000
    height: 850
    TrackListModel { id: tracks }
    TrackMenu { id: trackMenu; model: tracks }
    AlbumMenu { id: albumMenu }
    QueueMenu { id: playlistMenu; model: tracks; offlinePlaylistId: 17 }
    ServerMediaDeletion { id: host }
    SidebarPlaylistActions { id: sidebarActions; sidebar: test }
    QtObject {
        id: manager
        function folderIds() { return [] }
        function folderNames() { return [] }
    }
    PlaylistManagerActions { id: managerActions; manager: manager }
    SignalSpy { id: requested; target: Offline; signalName: "requested" }
    function init() {
        Offline.info = {count:2}
        Offline.menuCase = "none"
        Offline.status = {running:false,total:0}
        Offline.calls = []
        Navigation.page = ""
        requested.clear()
    }
    function cleanup() { trackMenu.close(); albumMenu.close(); playlistMenu.close(); managerActions.close() }
    // A menu's offline action runs where the listener is: it asks the facade
    // and neither opens a page nor asks for one.
    function choose(menu, act, key) {
        const action = menu.entries.find(entry => entry[key || "act"] === act)
        verify(action !== undefined, act)
        menu.actionChosen(action)
        compare(requested.count, 0)
        compare(Navigation.page, "")
    }
    function asked(action) { return Offline.calls.filter(call => call[0] === action).map(call => call[1] + " " + call[2]) }
    function test_track_menu_keeps_the_selected_remote_tracks() {
        trackMenu.show(2,0,0)
        choose(trackMenu, "offline:start")
        compare(asked("start"), ["tracks [42,43]"])
    }
    function test_album_menu_takes_the_whole_record() {
        albumMenu.show(17,"Record","Artist",0,0,0)
        choose(albumMenu, "offline:start")
        compare(asked("start"), ["album [17]"])
    }
    function test_playlist_header_takes_the_whole_playlist() {
        playlistMenu.show(0,0)
        choose(playlistMenu, "offline:start")
        compare(asked("start"), ["playlist [17]"])
    }
    function test_playlist_manager_and_sidebar_act_on_server_music() {
        const row = {rowKind:0,entryId:17,playlistId:17,server:true,name:"Remote order",local:false,writable:false}
        Offline.menuCase = "kept"
        managerActions.show(row,test,0,0)
        const index = managerActions.entries.findIndex(entry => entry.act === "offline:redownload")
        verify(index >= 0)
        managerActions.chosen(index)
        compare(asked("redownload"), ["playlist [17]"])
        sidebarActions.row(row,0,0)
        const menu = findChild(sidebarActions,"playlistsMenu")
        const sidebarIndex = menu.entries.findIndex(entry => entry.action === "offline:remove_now")
        verify(sidebarIndex >= 0)
        menu.chosen(sidebarIndex)
        compare(asked("remove_now"), ["playlist [17]"])
        compare(requested.count, 0)
        compare(Navigation.page, "")
        menu.close()
    }
    function test_the_menu_says_what_the_tracks_can_do_now() {
        const texts = () => trackMenu.entries.filter(entry => (entry.act || "").startsWith("offline:")).map(entry => entry.act)
        trackMenu.show(2,0,0)
        compare(texts(), ["offline:start"])
        trackMenu.close()
        Offline.menuCase = "kept"
        trackMenu.show(2,0,0)
        compare(texts(), ["offline:redownload", "offline:remove_now"])
        trackMenu.close()
        Offline.menuCase = "busy"
        trackMenu.show(2,0,0)
        compare(texts(), ["offline:cancel_selection"])
        choose(trackMenu, "offline:cancel_selection")
        compare(asked("cancel_selection"), ["tracks [42,43]"])
    }
    function test_local_only_music_does_not_offer_a_server_download() {
        Offline.info = {count:0}
        trackMenu.show(2,0,0)
        verify(!trackMenu.entries.some(entry => (entry.act || "").startsWith("offline:")))
        albumMenu.show(17,"Record","Artist",0,0,0)
        verify(!albumMenu.entries.some(entry => (entry.act || "").startsWith("offline:")))
    }
    function test_music_servers_alone_leads_to_the_offline_page() {
        Offline.requested("downloads", "[]")
        compare(Navigation.page, "offlineMusic")
        verify(!Offline.calls.some(call => call[0] === "start" || call[0] === "stage"))
        compare(findChild(test,"serverOfflineDialog"),null)
    }
}
