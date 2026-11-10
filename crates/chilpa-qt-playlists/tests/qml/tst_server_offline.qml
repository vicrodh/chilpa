import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
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
    ServerOfflineDialog { id: dialog; kind: "playlist"; selected: "[17]" }
    SidebarPlaylistActions { id: sidebarActions; sidebar: test }
    QtObject {
        id: manager
        function folderIds() { return [] }
        function folderNames() { return [] }
    }
    PlaylistManagerActions { id: managerActions; manager: manager }
    SignalSpy { id: requested; target: Sources; signalName: "offlineRequested" }
    function init() {
        Sources.offlineInfo = {count:2}
        Sources.offlineStatus = {running:false,total:0}
        Sources.offlineCalls = []
        requested.clear()
    }
    function cleanup() { dialog.close(); trackMenu.close(); albumMenu.close(); playlistMenu.close(); managerActions.close() }
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
        Sources.offlineInfo = {count:0}
        trackMenu.show(2,0,0)
        verify(!trackMenu.entries.some(entry => entry.act === "offline"))
    }
    function test_opening_only_reads_and_download_requires_its_button() {
        dialog.open()
        tryCompare(dialog,"opened",true)
        verify(Sources.offlineCalls.every(call => call[0] !== "start"))
        findChild(dialog,"startOfflineDownload").clicked()
        verify(Sources.offlineCalls.some(call => JSON.stringify(call) === '["start","playlist","[17]"]'))
        compare(dialog.busy,true)
        dialog.close()
        verify(!Sources.offlineCalls.some(call => call[0] === "cancel"))
    }
    function test_status_changes_keep_the_dialog_height_stable() {
        dialog.open()
        tryCompare(dialog,"opened",true)
        const original = dialog.height
        Sources.offlineStatus = {running:true,total:2,title:"Remote metadata ".repeat(120),done:0,failed:0}
        dialog.refresh()
        waitForRendering(test)
        compare(dialog.height,original)
        findChild(dialog,"cancelOfflineDownload").clicked()
        verify(Sources.offlineCalls.some(call => call[0] === "cancel"))
        compare(dialog.height,original)
    }
    function test_file_removal_requires_both_confirmations_and_the_acknowledgement() {
        dialog.open()
        tryCompare(dialog,"opened",true)
        findChild(dialog,"removeOfflineFiles").clicked()
        const confirmation = findChild(dialog,"offlineRemovalConfirmation")
        tryCompare(confirmation,"opened",true)
        const accept = findChild(confirmation,"confirmationAccept")
        accept.clicked()
        verify(!Sources.offlineCalls.some(call => call[0] === "remove"))
        const check = findChild(confirmation,"destructiveAcknowledgement")
        compare(accept.enabled,false)
        check.checked = true
        tryCompare(accept,"enabled",true)
        accept.clicked()
        verify(Sources.offlineCalls.some(call => call[0] === "remove" && call[2] === "fixture-removal"))
        compare(dialog.busy,true)
    }
}
