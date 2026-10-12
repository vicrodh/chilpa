import QtQuick
import QtTest
import "../../qml/Screens"

TestCase {
    name: "PlaylistManagerActions"
    when: windowShown
    visible: true
    width: 960
    height: 600
    PlaylistManagerScreen { id: screen; anchors.fill: parent }

    function test_edit_is_reachable_from_grid_and_list() {
        for (const grid of [true, false]) {
            screen.grid = grid
            wait(0)
            const button = findChild(screen, "playlistMore")
            verify(button !== null)
            waitForRendering(button)
            mouseClick(button, button.width / 2, button.height / 2)
            const menu = findChild(button.parent.parent, "playlistActions")
            verify(menu !== null)
            tryCompare(menu, "opened", true)
            let edit = null
            for (const item of menu.entryItems)
                if (item.entry !== undefined && item.entry.act === "edit-playlist") edit = item
            verify(edit !== null)
            waitForRendering(edit)
            mouseClick(edit, edit.width / 2, edit.height / 2)
            const dialog = findChild(screen, "playlistEditDialog")
            tryCompare(dialog, "opened", true)
            compare(findChild(dialog.contentItem, "playlistName").text, "Stored playlist")
            dialog.close()
        }
    }
}
