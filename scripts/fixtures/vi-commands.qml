import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

ApplicationWindow {
    id: window
    width: 1100; height: 700; visible: true
    property int fixtureAlbum: 0
    property int phase: 0
    property string lastQuery: ""
    property string lastSource: ""
    property int resultSteps: 0
    TrackListModel { id: tracks; kind: "tracks"; key: "album\u001foff\u001fVi smoke fixture" }
    TrackList { id: list; width: parent.width; height: 500; model: tracks; rich: false }
    TextInput { id: field; y: 520; width: 300; height: 30 }
    SearchBox { id: searchBox; y: 560; width: 300; room: 400 }
    WindowControls { id: controls; window: window }
    TestCase { id: keyboard; when: false }
    Connections {
        target: Hotkeys
        function onSearchCommand(query, source) { window.lastQuery = query; window.lastSource = source }
        function onSearchStepped(by) { window.resultSteps += by }
    }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) { const found = find(child, name); if (found) return found }
        return null
    }
    function check(condition, message) {
        if (!condition) { console.log("SMOKE FAIL " + message); Qt.exit(1); throw new Error(message) }
    }
    function press(key, text, modifiers) {
        const event = { key: key, text: text || "", modifiers: modifiers || 0, accepted: false }
        controls.pressed(event)
        return event.accepted
    }
    function command(input) {
        const result = JSON.parse(CommandLine.prepare(input))
        check(!result.error, input + ": " + (result.error || ""))
        controls.run(result.action, result)
        return result
    }
    Timer {
        interval: 100; repeat: true; running: true
        onTriggered: {
            if (!tracks.ready || tracks.count < 100) return
            if (window.phase === 0) {
                Hotkeys.command("keymap", "vi")
                list.forceActiveFocus()
                list.viAction("first")
                window.check(list.cursorRow === 0, "gg selects the first native track")
                window.press(Qt.Key_5, "5"); window.press(Qt.Key_J, "j")
                window.check(list.cursorRow === 5, "5j advances five native rows")
                window.press(Qt.Key_G, "g"); window.press(Qt.Key_G, "g")
                window.check(list.cursorRow === 0, "gg resolves through the shared action table")
                window.press(Qt.Key_G, "G", Qt.ShiftModifier)
                window.check(list.cursorRow === 99, "G selects the final native row")
                window.press(Qt.Key_U, "u", Qt.ControlModifier)
                window.check(list.cursorRow < 99, "Ctrl+u moves half a page")
                field.forceActiveFocus()
                const before = list.cursorRow
                window.check(!window.press(Qt.Key_J, "j") && list.cursorRow === before, "normal keys belong to text fields")
                window.press(Qt.Key_Escape, "")
                window.check(list.activeFocus, "Escape returns focus to the track list")
                controls.immersiveOpen = true
                window.press(Qt.Key_J, "j")
                window.check(list.cursorRow === before, "covered lists ignore immersive normal keys")
                controls.immersiveOpen = false
                console.log("SMOKE vi native counts cursor focus passed")
            } else if (window.phase === 1) {
                const bar = window.find(window.contentItem, "commandBar")
                window.check(!!bar, "command bar is attached to the real window")
                bar.show("unknown-command", list)
                bar.submit()
                window.check(bar.opened && bar.error !== "" && bar.input.activeFocus, "invalid command stays editable with an inline error")
                bar.dismiss()
                window.check(list.activeFocus, "closing the command bar restores the native list")
                window.check(JSON.parse(CommandLine.complete("search:l")).includes("search:local "), "source ids complete")
                window.command("/search:local Vi smoke fixture")
                window.check(window.lastSource === "local" && window.lastQuery === "Vi smoke fixture", "source command reaches the existing search flyout")
                list.forceActiveFocus()
                keyboard.keyClick(Qt.Key_N)
                window.check(window.resultSteps === 1, "n steps the existing search forward")
                keyboard.keyClick(Qt.Key_N, Qt.ShiftModifier)
                window.check(window.resultSteps === 0, "n and N step the same search results")
                const seek = JSON.parse(CommandLine.prepare(":seek 1:30"))
                window.check(seek.value === 90 && !seek.relative, "clock seek parses to seconds")
                window.check(!!JSON.parse(CommandLine.prepare(":seek 1e308:0")).error, "overflow seek is rejected")
                window.check(!!JSON.parse(CommandLine.prepare(":set appearance.density broken")).error, "preference choices are validated")
                window.command(":set appearance.density compact")
                window.check(SettingsModel.density === "compact", "set uses the existing settings action")
                window.check(JSON.parse(CommandLine.history()).includes(":set appearance.density compact"), "history persists successful commands")
                console.log("SMOKE vi command errors completion source search and settings passed")
            } else if (window.phase === 2) {
                Navigation.open("library", 0)
                const second = Navigation.active
                list.forceActiveFocus()
                window.press(Qt.Key_G, "g"); window.press(Qt.Key_T, "t")
                window.check(Navigation.active !== second, "gt selects the next tab")
                window.press(Qt.Key_G, "g"); window.press(Qt.Key_T, "T", Qt.ShiftModifier)
                window.check(Navigation.active === second, "gT selects the previous tab")
                window.command(":q")
                window.command(":sleep 30")
                window.check(Controls.sleepLeft !== "", "sleep starts the real timer")
                window.command(":sleep off")
                window.check(Controls.sleepLeft === "", "sleep off cancels the real timer")
                window.command(":queue save Vi smoke saved queue")
                window.command(":queue clear")
                ListenList.refresh()
                const queue = JSON.parse(ListenList.snapshot)
                window.check(queue.tabs.find(tab => tab.id === queue.selected).count === 0, "queue clear edits the selected real queue")
                console.log("SMOKE vi tabs sleep save and clear passed")
                console.log("SMOKE vi commands completed")
                Qt.exit(0)
            }
            ++window.phase
        }
    }
    Timer { interval: 20000; running: true; onTriggered: { console.log("SMOKE FAIL vi commands timed out phase=" + window.phase); Qt.exit(1) } }
}
