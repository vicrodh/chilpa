import QtQuick
import QtTest
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns" as SourceUi
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
    SettingsRow { id: format; y: 180; width: parent.width; rowKind: 1; key: "integrations.discord.titleFormat"; title: "Title"; description: ""; control: "text"; action: ""; value: "%title%"; options: []; optionValues: []; marks: []; page: actions }
    SettingsRow { id: runtime; y: 270; width: parent.width; rowKind: 1; key: "metadata.beetsPython"; title: "Custom Python executable"; description: ""; control: "file"; action: "Choose executable"; value: "/fixture/previous python"; options: ["All files (*)"]; optionValues: []; marks: []; page: actions }
    StorageSettingsActions { id: storage; rows: actions }
    Component {
        id: hiddenManagerFactory
        SourceUi.PluginManager {
            visible:false; width:600; height:600; page:probe
            readonly property int backendReads:probe.reads
            QtObject {
                id:probe
                property int reads:0
                function pluginsJson() { reads++;return '{"installed":[],"catalog":[],"busy":false}' }
                function pluginCommand(action,input) {}
            }
        }
    }
    function test_hidden_managers_wait_until_shown_and_stop_when_hidden_again() {
        const hidden=createTemporaryObject(hiddenManagerFactory,this)
        wait(Motion.poll*2)
        compare(hidden.backendReads,0)
        hidden.visible=true
        tryVerify(()=>hidden.backendReads>0)
        hidden.visible=false
        const before=hidden.backendReads
        wait(Motion.poll*2)
        compare(hidden.backendReads,before)
    }
    function init() {
        writes = []; presses = []; backgroundClicks = 0
        credits.value = "false"; PlayerLayout.mode = "floating"; runtime.control = "file"
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
    function test_a_text_row_keeps_what_is_typed_on_enter() {
        const field = findChild(format, "settingText.integrations.discord.titleFormat")
        compare(field.text, "%title%"); field.focusInput(); keyClick(Qt.Key_X); keyClick(Qt.Key_Return)
        compare(writes[0], ["integrations.discord.titleFormat", "x"]); format.value = "x"
    }
    function test_an_executable_picker_forwards_the_path_and_can_restore_the_default() {
        const field = findChild(runtime, "settingFile.metadata.beetsPython"), selected = "/fixture/Curated library/#venv/bin/python"
        compare(field.filters, ["All files (*)"]); compare(field.dialogFilters(), "[]", "every file: no filter for the dialog")
        field.picked(selected); compare(writes, [["metadata.beetsPython", selected]])
        field.cleared(); compare(writes[1], ["metadata.beetsPython", ""])
        compare(field.enabled, true); runtime.control = "fileBusy"; compare(field.enabled, false)
    }
    function test_lyrics_cleanup_dispatches_only_after_confirmation_data() {
        return [{tag:"storage",key:"storage.clear.lyrics"},{tag:"library",key:"library.lyricsCache"}]
    }
    function test_lyrics_cleanup_dispatches_only_after_confirmation(data) {
        compare(storage.press(data.key), true)
        const dialog = findChild(storage, "storageClearConfirmation")
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Clear lyrics cache?")
        compare(presses.length, 0)
        mouseClick(test, 795, 515)
        tryCompare(dialog, "visible", false)
        compare(backgroundClicks, 0)
        compare(presses.length, 0)
        storage.press(data.key)
        tryCompare(dialog, "opened", true)
        const accept = findChild(dialog.contentItem,"confirmationAccept")
        accept.clicked()
        compare(presses.length,0)
        findChild(dialog.contentItem,"destructiveAcknowledgement").checked = true
        accept.clicked()
        compare(presses, ["storage.clear.lyrics"])
        compare(storage.press("library.rescan"), false)
    }
}
