import QtQuick
import QtTest
import app.chilpa.controls
import "../../qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

TestCase {
    id: test
    name: "SettingsControls"
    when: windowShown
    visible: true
    width: 800
    height: 520
    property var writes: []
    property var presses: []
    property int backgroundClicks: 0
    MouseArea { anchors.fill: parent; onClicked: test.backgroundClicks++ }
    QtObject {
        id: actions
        property string editingServerKey: ""
        function set(key, value) { test.writes.push([key, value]); credits.value = value }
        function press(key) { test.presses.push(key) }
        function refresh() {}
    }
    SettingsRow {
        id: layout
        width: parent.width
        rowKind: 1; key: "navigation.playerLayout"; title: "Player bar position"
        description: ""; control: "select"; action: ""; value: "floating"
        options: ["Floating", "Full width bottom", "Full width top"]
        optionValues: ["floating", "bottom", "top"]; marks: []; page: actions
    }
    SettingsRow {
        id: credits
        y: 90; width: parent.width
        rowKind: 1; key: "navigation.trackComposers"; title: "Show composers"
        description: ""; control: "toggle"; action: ""; value: "false"
        options: []; optionValues: []; marks: []; page: actions
    }
    StorageSettingsActions { id: storage; rows: actions }
    function init() {
        writes = []; presses = []; backgroundClicks = 0
        credits.value = "false"; PlayerLayout.mode = "floating"
    }
    function cleanup() {
        const dialog = findChild(storage, "storageClearConfirmation")
        dialog.close(); tryCompare(dialog, "visible", false)
    }
    function test_layout_mirrors_player_in_both_directions() {
        const select = findChild(layout, "settingSelect.navigation.playerLayout")
        compare(select.values.length, 3)
        select.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(PlayerLayout.mode, "bottom")
        keyClick(Qt.Key_Down)
        compare(PlayerLayout.mode, "top")
        PlayerLayout.mode = "floating"
        compare(select.value, "floating")
        compare(select.current, 0)
        compare(writes.length, 0)
    }
    function test_composers_require_an_explicit_opt_in() {
        const toggle = findChild(credits, "settingToggle.navigation.trackComposers")
        compare(toggle.checked, false)
        mouseClick(toggle)
        compare(writes.length, 1)
        compare(writes[0], ["navigation.trackComposers", "true"])
        compare(toggle.checked, true)
    }
    function test_lyrics_cleanup_dispatches_only_after_confirmation() {
        compare(storage.press("storage.clear.lyrics"), true)
        const dialog = findChild(storage, "storageClearConfirmation")
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Clear lyrics cache?")
        compare(presses.length, 0)
        mouseClick(test, 795, 515)
        tryCompare(dialog, "visible", false)
        compare(backgroundClicks, 0)
        compare(presses.length, 0)
        storage.press("storage.clear.lyrics")
        tryCompare(dialog, "opened", true)
        dialog.confirmed()
        compare(presses, ["storage.clear.lyrics"])
        compare(storage.press("library.rescan"), false)
    }
}
