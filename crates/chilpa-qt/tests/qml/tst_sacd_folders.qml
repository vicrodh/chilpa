import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Screens"

TestCase {
    name: "SacdFolderOpening"
    when: windowShown
    visible: true
    width: 1280
    height: 720
    FoldersScreen { id: screen; anchors.fill: parent }

    function test_picker_is_reachable_from_both_folder_modes() {
        const model = findChild(screen, "folderContents")
        const button = findChild(screen, "openSacd")
        verify(model !== null)
        verify(button !== null)
        for (const flat of [false, true]) {
            screen.flat = flat
            waitForRendering(button)
            const before = model.discPicks
            mouseClick(button, button.width / 2, button.height / 2)
            compare(model.discPicks, before + 1)
            compare(screen.flat, flat, "cancelling a picker does not change the view")
            model.folderOpened()
            compare(screen.flat, false, "the selected image opens its tracklist")
            compare(SettingsModel.kept("ui.folders.mode"), "tree")
        }
    }
}
