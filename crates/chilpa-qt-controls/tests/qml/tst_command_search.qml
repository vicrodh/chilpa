import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Patterns"
import "../../qml/Components"
import "../../../chilpa-qt/qml/Patterns"

TestCase {
    id: test
    name: "CommandSearchKeyboard"
    when: windowShown
    visible: true
    width: 1100
    height: 700
    WindowControls { id: controls; window: test.Window.window }
    Item { id: origin; width: 10; height: 10 }
    Item { id: host; property bool compact: false }
    TextInput { id: field; y: 650; width: 300; height: 30 }
    SearchModel {
        id: model
        property int actedRow: -1
        function act(row, action) { actedRow = row }
    }
    SearchFlyout { id: flyout; search: model; room: 600 }
    SearchCommands { host: host; search: model; field: field; flyout: flyout }

    function init() {
        Hotkeys.snapshot = JSON.stringify({keymap:"vi"})
        Hotkeys.answer = ""
        Hotkeys.searchActive = false
        CommandLine.choices = []
        CommandLine.saved = []
        model.clear()
        model.text = ""
        model.actedRow = -1
        flyout.current = -1
        for (let i = 0; i < 6; ++i)
            model.append({kind:i % 2 === 0 ? 0 : 4, title:"Result " + i, detail:"Album", art:"", duration:"3:00", sourceName:"", sourceIcon:""})
        origin.forceActiveFocus()
    }
    function cleanup() {
        flyout.close()
        findChild(controls, "commandBar").opened = false
        Hotkeys.answer = ""
    }
    function openSearch(opener) {
        Hotkeys.answer = opener === "colon" ? "commands.open" : "nav.search"
        keyClick(opener === "colon" ? Qt.Key_Colon : Qt.Key_Slash, opener === "colon" ? Qt.ShiftModifier : Qt.NoModifier)
        const bar = findChild(controls, "commandBar")
        verify(bar.opened && bar.input.activeFocus)
        compare(bar.input.text, opener === "colon" ? "" : "search ")
        Hotkeys.answer = ""
        for (const character of (opener === "colon" ? "search Album" : "Album")) keyClick(character)
        CommandLine.answer = JSON.stringify({action:"nav.search", argument:"Album", target:"local"})
        keyClick(Qt.Key_Return)
        tryCompare(flyout, "opened", true)
        waitForRendering(flyout.contentItem)
        verify(!bar.opened)
        compare(model.text, "Album")
        compare(model.source, "local")
        verify(Hotkeys.searchActive)
    }
    function test_command_results_take_arrows_and_enter_data() {
        return [{tag:"colon", opener:"colon"}, {tag:"slash", opener:"slash"}]
    }
    function test_command_results_take_arrows_and_enter(data) {
        openSearch(data.opener)
        keyClick(Qt.Key_Down)
        compare(flyout.current, 1, "down skips the first section header")
        keyClick(Qt.Key_Down)
        compare(flyout.current, 3, "down skips the next section header")
        keyClick(Qt.Key_Up)
        compare(flyout.current, 1)
        keyClick(Qt.Key_Return)
        compare(model.actedRow, 1)
        tryCompare(flyout, "opened", false)
        verify(!Hotkeys.searchActive)
    }
    function test_escape_restores_the_command_origin_without_clearing_the_query() {
        openSearch("colon")
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Escape)
        tryCompare(flyout, "opened", false)
        verify(origin.activeFocus)
        compare(model.text, "Album")
        compare(model.actedRow, -1)
    }
    function test_reopened_search_takes_arrows_and_enter() {
        openSearch("colon")
        keyClick(Qt.Key_Down)
        compare(flyout.current, 1)
        keyClick(Qt.Key_Escape)
        tryCompare(flyout, "opened", false)
        verify(origin.activeFocus)
        Hotkeys.answer = "search.next"
        keyClick(Qt.Key_N)
        Hotkeys.answer = ""
        tryCompare(flyout, "opened", true)
        keyClick(Qt.Key_Down)
        compare(flyout.current, 5)
        keyClick(Qt.Key_Return)
        compare(model.actedRow, 5)
        tryCompare(flyout, "opened", false)
    }
}
