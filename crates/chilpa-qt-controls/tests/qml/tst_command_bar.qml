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
