import QtQuick
import QtTest
import app.chilpa
import app.chilpa.devices
import "../../../chilpa-qt/qml/Components"

TestCase {
    name: "AlbumMenuDeviceSync"
    when: windowShown
    visible: true
    width: 960
    height: 600

    AlbumMenu { id: menu }
    SignalSpy { id: quickSpy; target: menu; signalName: "quick" }

    function entry(action) {
        for (const item of menu.entryItems)
            if (item.entry !== undefined && item.entry.act === action) return item
        return null
    }

    function init() {
        AlbumActions.selectedIds = []
        AlbumActions.selectedCount = 0
        AlbumActions.menuTrackIdsJson = "[]"
        AlbumActions.requestedAlbumId = 0
        DeviceSync.lastAction = []
        Navigation.page = "albums"
        Library.apart = ({})
        Library.keptApart = []
        menu.canExpand = false
        quickSpy.clear()
    }

    function cleanup() { menu.close() }

    function test_sync_click_preserves_every_track_and_opens_review_data() {
        return [
            {tag: "whole_multidisc_album", selected: [], tracks: "[81,82,91,92]"},
            {tag: "whole_album_selection", selected: [7,9], tracks: "[81,82,91,92,101,102]"}
        ]
    }

    function test_sync_click_preserves_every_track_and_opens_review(data) {
        AlbumActions.selectedIds = data.selected
        AlbumActions.selectedCount = data.selected.length
        AlbumActions.menuTrackIdsJson = data.tracks
        menu.show(7, "Record", "Artist", 0, 100, 100)
        tryCompare(menu, "opened", true)
        const sync = entry("sync")
        verify(sync !== null)
        compare(sync.entry.text, "Sync to device…")
        compare(AlbumActions.requestedAlbumId, 0)
        compare(DeviceSync.lastAction, [])
        compare(Navigation.page, "albums")
        if (data.selected.length > 1) verify(menu.entries[0].note)
        waitForRendering(sync)
        mouseClick(sync, sync.width / 2, sync.height / 2)
        compare(AlbumActions.requestedAlbumId, 7)
        compare(DeviceSync.lastAction, ["tracks",data.tracks])
        compare(Navigation.page, "deviceSync")
        compare(Navigation.selectedId, 0)
        tryCompare(menu, "visible", false)
    }

    function test_remote_and_empty_contexts_do_not_offer_sync_data() {
        return [{tag:"source_album",id:-7},{tag:"no_album",id:0}]
    }

    function test_remote_and_empty_contexts_do_not_offer_sync(data) {
        menu.show(data.id, "Record", "Artist", 0, 100, 100)
        tryCompare(menu, "opened", true)
        compare(entry("sync"), null)
        compare(AlbumActions.requestedAlbumId, 0)
        compare(DeviceSync.lastAction, [])
        compare(Navigation.page, "albums")
    }

    function test_grouping_action_keeps_the_intent_shown_in_its_label() {
        Library.apart = ({7:0})
        menu.show(7, "Record", "Artist", 0, 100, 100)
        const apart = menu.entries.find(item => item.act === "apart")
        compare(apart.text, "Don't group with other versions")
        Library.apart = ({7:1})
        menu.actionChosen(apart)
        compare(Library.keptApart, [[7,true]])
    }

    function test_quick_view_and_artist_keep_their_menu_context() {
        menu.canExpand = true
        menu.show(7, "Record", "Artist", 4, 100, 100)
        menu.actionChosen(menu.entries.find(item => item.act === "quick"))
        compare(quickSpy.count, 1)
        compare(quickSpy.signalArguments[0][0], 4)
        menu.actionChosen(menu.entries.find(item => item.act === "artist"))
        compare(Navigation.went, ["artist","Artist"])
    }
}
