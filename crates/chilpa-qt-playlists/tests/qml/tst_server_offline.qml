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
        Offline.status = {running:false,total:0}
        Offline.calls = []
        requested.clear()
    }
    function cleanup() { trackMenu.close(); albumMenu.close(); playlistMenu.close(); managerActions.close() }
    function choose(menu) {
        const action = menu.entries.find(entry => entry.act === "offline")
        verify(action !== undefined)
        menu.actionChosen(action)
        compare(requested.count,1)
    }
    function test_track_menu_keeps_the_selected_remote_tracks() {
        trackMenu.show(2,0,0)
        choose(trackMenu)
        compare(requested.signalArguments[0][0],"tracks")
        compare(requested.signalArguments[0][1],"[42,43]")
    }
    function test_album_menu_opens_the_whole_record() {
        albumMenu.show(17,"Record","Artist",0,0,0)
        choose(albumMenu)
        compare(requested.signalArguments[0][0],"album")
        compare(requested.signalArguments[0][1],"[17]")
    }
    function test_playlist_header_opens_the_whole_playlist() {
        playlistMenu.show(0,0)
        choose(playlistMenu)
        compare(requested.signalArguments[0][0],"playlist")
        compare(requested.signalArguments[0][1],"[17]")
    }
    function test_playlist_manager_and_sidebar_offer_offline_for_server_music() {
        const row = {rowKind:0,entryId:17,playlistId:17,server:true,name:"Remote order",local:false,writable:false}
        managerActions.show(row,test,0,0)
        const index = managerActions.entries.findIndex(entry => entry.act === "offline")
        verify(index >= 0)
        managerActions.chosen(index)
        compare(requested.signalArguments[0][0],"playlist")
        compare(requested.signalArguments[0][1],"[17]")
        requested.clear()
        sidebarActions.row(row,0,0)
        const menu = findChild(sidebarActions,"playlistsMenu")
        const sidebarIndex = menu.entries.findIndex(entry => entry.action === "offline")
        verify(sidebarIndex >= 0)
        menu.chosen(sidebarIndex)
        compare(requested.signalArguments[0][0],"playlist")
        compare(requested.signalArguments[0][1],"[17]")
        menu.close()
    }
    function test_local_only_music_does_not_offer_a_server_download() {
        Offline.info = {count:0}
        trackMenu.show(2,0,0)
        verify(!trackMenu.entries.some(entry => entry.act === "offline"))
    }
    // The window's host takes every menu to the Offline music page with its
    // choice staged; no dialog is opened for it.
    function test_a_menu_choice_opens_the_offline_page_with_it_staged() {
        Navigation.page = ""
        albumMenu.show(17,"Record","Artist",0,0,0)
        choose(albumMenu)
        compare(Navigation.page,"offlineMusic")
        verify(Offline.calls.some(call => JSON.stringify(call) === '["stage","album","[17]"]'))
        compare(Offline.selection.kind,"album")
        verify(!Offline.calls.some(call => call[0] === "start"), "choosing only opens the page")
        compare(findChild(test,"serverOfflineDialog"),null)
    }
}
