import QtQuick
import QtTest
import app.chilpa.controls
import "../../qml/Patterns"

TestCase {
    id: test
    name: "CommandBar"
    when: windowShown
    visible: true
    width: 900
    height: 500
    property var executed: null
    property var dismissed: null
    Item { id: previous; width: 10; height: 10 }
    FontMetrics { id: commandFont; font: bar.input.font }
    CommandBar {
        id: bar
        width: test.width
        y: test.height - height
        onExecute: command => test.executed = command
        onDismissed: item => test.dismissed = item
    }
    function init() {
        executed = null
        dismissed = null
        CommandLine.answer = "{}"
        CommandLine.saved = ["search first", "vol 40"]
        CommandLine.choices = ["search ", "set "]
        bar.show("", previous)
    }
    function cleanup() { bar.opened = false }
    function typeText(text) { for (const character of text) keyClick(character) }
    function test_command_text_uses_a_fixed_width_font() {
        verify(Math.abs(commandFont.advanceWidth("iiii") - commandFont.advanceWidth("WWWW")) < 0.1)
    }
    function test_unknown_command_keeps_the_line_open_and_editable() {
        bar.input.text = "unknown"
        CommandLine.answer = JSON.stringify({ error: "Comando desconocido" })
        keyClick(Qt.Key_Return)
        verify(bar.opened)
        compare(bar.error, "Comando desconocido")
        compare(executed, null)
        verify(bar.input.activeFocus)
        typeText(" corrected")
        compare(bar.error, "")
    }
    function test_success_uses_the_shared_action_and_restores_focus() {
        CommandLine.answer = JSON.stringify({ action: "playback.volume", value: 40, relative: false })
        bar.input.text = "vol 40"
        keyClick(Qt.Key_Return)
        verify(!bar.opened)
        compare(executed.action, "playback.volume")
        compare(executed.value, 40)
        compare(dismissed, previous)
    }
    function test_help_stays_in_the_console_and_escape_returns_to_the_prompt() {
        CommandLine.answer = JSON.stringify({action:"commands.help",lines:[{usage:":seek seconds",description:"Seek"}]})
        bar.input.text = "help seek"
        keyClick(Qt.Key_Return)
        verify(bar.opened && bar.input.activeFocus)
        compare(executed, null)
        compare(dismissed, null)
        const help = findChild(bar, "commandHelpLines")
        verify(help !== null)
        compare(help.count, 1)
        keyClick(Qt.Key_Escape)
        verify(bar.opened && bar.input.activeFocus)
        compare(bar.helpLines.length, 0)
        keyClick(Qt.Key_Escape)
        verify(!bar.opened)
        compare(dismissed, previous)
    }
    function test_a_closed_console_keeps_no_keys_when_the_root_takes_them_back() {
        keyClick(Qt.Key_Escape)
        verify(!bar.opened)
        // The window hands the keys to its root, a focus scope, as the window's
        // restore does when the console was opened from nowhere in particular.
        test.Window.window.contentItem.forceActiveFocus()
        verify(!bar.input.activeFocus, "the next colon opens the console again")
    }
    function test_history_preserves_the_unsubmitted_draft() {
        typeText("search draft")
        keyClick(Qt.Key_Up)
        compare(bar.input.text, "vol 40")
        keyClick(Qt.Key_Up)
        compare(bar.input.text, "search first")
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Down)
        compare(bar.input.text, "search draft")
    }
    function test_completion_and_escape_belong_to_the_bar() {
        typeText("se")
        keyClick(Qt.Key_Tab)
        compare(bar.input.text, "search ")
        keyClick(Qt.Key_Tab)
        compare(bar.input.text, "set ")
        keyClick(Qt.Key_Escape)
        verify(!bar.opened)
        compare(dismissed, previous)
        compare(executed, null)
    }
}
