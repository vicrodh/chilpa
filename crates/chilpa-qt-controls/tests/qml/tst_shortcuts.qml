import QtQuick
import QtTest
import app.chilpa.controls
import "../../qml/Patterns"

TestCase {
    id: test
    name: "ShortcutUsability"
    when: windowShown
    visible: true
    width: 1200
    height: 550
    WindowControls { id: controls; window: test.Window.window }
    Item { id: origin; width: 10; height: 10 }
    readonly property var groups: ({
        col1:[{label:"Playback", rows:Array.from({length:24}, (_, i) => ({id:"playback." + i,
            label:"Playback action " + i, shortcut:i === 0 ? "Space" : i > 2 ? "Ctrl + " + i : "", command:i === 1 ? ":play" : ""}))}],
        col2:[{label:"Vi", rows:[{id:"list.down", label:"Cursor down", shortcut:"", command:""}]}],
        col3:[{label:"Interface", rows:[{id:"commands.open", label:"Command line", shortcut:":"}]}],
        keymap:"default", recording:"", tried:"", takenBy:""})

    function caps(root) {
        const result = []
        function walk(item) {
            for (const child of item.children) {
                if (child.objectName === "keycap") result.push(child)
                walk(child)
            }
        }
        walk(root)
        return result
    }
    function popup(name) {
        const value = findChild(controls, name)
        value.open()
        tryVerify(() => value.opened)
        waitForRendering(value.contentItem)
        return value
    }
    function init() {
        Hotkeys.snapshot = JSON.stringify(groups)
        Hotkeys.answer = ""
        Hotkeys.commands = []
        Hotkeys.keyEvents = []
        origin.forceActiveFocus()
    }
    function cleanup() {
        findChild(controls, "shortcutsSheet").close()
        findChild(controls, "shortcutsEditor").close()
        findChild(controls, "commandBar").opened = false
    }
    function test_guide_has_no_empty_key_badges_and_reserves_scrollbar_space() {
        const sheet = popup("shortcutsSheet")
        const shown = caps(sheet.contentItem).filter(cap => cap.visible)
        verify(shown.length > 20)
        verify(shown.every(cap => cap.text !== ""))
        const rows = [], groups = []
        function collect(item) {
            for (const child of item.children) {
                if (child.objectName === "shortcutRow") rows.push(child)
                if (child.objectName === "shortcutGroup") groups.push(child)
                collect(child)
            }
        }
        collect(sheet.contentItem)
        verify(rows.every(row => (row.entry.shortcut || "") !== ""))
        verify(!rows.some(row => row.entry.command === ":play"), "command-only actions belong to console help")
        verify(!rows.some(row => row.entry.id === "playback.2"))
        verify(groups.every(group => group.modelData.rows.length > 0))
        verify(!groups.some(group => group.modelData.label === "Vi"), "inactive groups leave no empty headings")
        const columns = findChild(sheet.contentItem, "shortcutColumns")
        const bar = findChild(sheet.contentItem, "shortcutScrollBar")
        verify(bar.size < 1, "the short modal actually needs scrolling")
        verify(bar.x >= columns.width, "the scrollbar has its own gutter beside all columns")
        verify(bar.x + bar.width <= columns.parent.width, "the thumb stays inside the viewport")
    }
    function test_editor_labels_unassigned_keys_and_captures_colon_inside_modal() {
        const editor = popup("shortcutsEditor")
        compare(caps(editor.contentItem).length, 26, "the editor keeps every action, including inactive groups")
        const unassigned = caps(editor.contentItem).find(cap => cap.text === "Unassigned")
        verify(unassigned && unassigned.visible)
        mouseClick(unassigned)
        const doc = JSON.parse(Hotkeys.snapshot)
        doc.recording = "playback.1"
        Hotkeys.snapshot = JSON.stringify(doc)
        tryVerify(() => editor.contentItem.activeFocus)
        Hotkeys.answer = "-"
        Hotkeys.keyEvents = []
        keyClick(Qt.Key_Colon, Qt.ShiftModifier)
        const captured = Hotkeys.keyEvents.filter(event => event.key === Qt.Key_Colon)
        compare(captured.length, 1, "the modal records the printable key exactly once")
        compare(captured[0].text, ":")
        verify(captured[0].modifiers & Qt.ShiftModifier)
        verify(editor.opened)
        verify(!findChild(controls, "commandBar").opened)
    }
    function test_colon_opens_command_input_and_escape_restores_focus_data() {
        return [{tag:"default", keymap:"default"}, {tag:"letters", keymap:"letters"}, {tag:"vi", keymap:"vi"}]
    }
    function test_colon_opens_command_input_and_escape_restores_focus(data) {
        const doc = JSON.parse(Hotkeys.snapshot)
        doc.keymap = data.keymap
        Hotkeys.snapshot = JSON.stringify(doc)
        Hotkeys.answer = "commands.open"
        keyClick(Qt.Key_Colon, Qt.ShiftModifier)
        const bar = findChild(controls, "commandBar")
        verify(bar.opened)
        verify(bar.input.activeFocus)
        compare(bar.input.text, "", "the opener is not inserted into the command")
        Hotkeys.answer = ""
        keyClick(Qt.Key_Escape)
        verify(!bar.opened)
        verify(origin.activeFocus)
    }
}
