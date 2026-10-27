import QtQuick
import QtTest
import app.chilpa
import app.chilpa.devices
import "../../../chilpa-qt-sources/qml/Patterns"

TestCase {
    name: "AlbumSelectionDevice"
    when: windowShown
    visible: true
    width: 960
    height: 600
    SelectionBar { id: bar; anchors.centerIn: parent }
    function cleanup() { AlbumActions.clearSelection() }
    function test_local_mixed_and_external_selection_data() {
        return [
            {tag: "local", ids: [7,9], enabled: true},
            {tag: "mixed", ids: [7,-9], enabled: true},
            {tag: "external", ids: [-7,-9], enabled: false}
        ]
    }
    function test_local_mixed_and_external_selection(data) {
        Navigation.page = "albums"
        DeviceSync.lastAction = []
        AlbumActions.selectedIds = data.ids
        AlbumActions.selectedCount = data.ids.length
        ++AlbumActions.rev
        AlbumActions.menuTrackIdsJson = "[81,82,91,92]"
        const button = findChild(bar, "albumSelectionDevice")
        verify(button !== null)
        compare(button.enabled, data.enabled)
        mouseClick(button, button.width / 2, button.height / 2)
        compare(Navigation.page, data.enabled ? "deviceSync" : "albums")
        compare(DeviceSync.lastAction, data.enabled ? ["tracks","[81,82,91,92]"] : [])
    }
}
