import QtQuick
import QtTest
import app.chilpa.playlists
import app.chilpa
import app.chilpa.devices
import "../../qml/Components"

TestCase {
    name: "TrackMenuActions"
    when: windowShown
    visible: true
    width: 960
    height: 600
    TrackListModel { id: tracks }
    TrackMenu { id: menu; model: tracks }

    function entry(action) {
        for (const item of menu.entryItems)
            if (item.entry !== undefined && item.entry.act === action) return item
        return null
    }
    function cleanup() { menu.close() }

    function test_play_later_click_dispatches_the_target_row_and_closes() {
        menu.show(3, 100, 100)
        tryCompare(menu, "opened", true)
        const later = entry("later")
        verify(later !== null)
        waitForRendering(later)
        mouseClick(later, later.width / 2, later.height / 2)
        compare(tracks.lastAction, "later")
        compare(tracks.lastRow, 3)
        tryCompare(menu, "visible", false)
    }

    function test_playlist_action_is_a_dialog_not_a_submenu() {
        menu.show(2, 100, 100)
        tryCompare(menu, "opened", true)
        const pick = entry("pick")
        verify(pick !== null)
        verify(!pick.entry.submenu)
        waitForRendering(pick)
        mouseClick(pick, pick.width / 2, pick.height / 2)
        tryCompare(menu, "visible", false)
        compare(AddToPlaylist.lastAsked, "[42,43]", "the window's one picker is asked for the tracks")
    }
    function test_device_action_keeps_the_complete_selection_and_opens_its_review() {
        menu.show(2,100,100)
        tryCompare(menu,"opened",true)
        const sync = entry("sync")
        verify(sync !== null)
        waitForRendering(sync)
        mouseClick(sync,sync.width / 2,sync.height / 2)
        compare(DeviceSync.lastAction,["tracks","[42,43]"])
        compare(Navigation.page,"deviceSync")
        tryCompare(menu,"visible",false)
    }
}
