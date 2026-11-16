import QtQuick
import QtTest
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "PluginRow"
    when: windowShown
    visible: true
    width: 500
    height: 100
    property var changes: []
    PluginRow { id: row; width: 500; installed: true; plugin: ({}); onSwitched: on => changes.push(on) }
    function test_an_enabled_failing_worker_can_be_switched_off() {
        row.plugin = {name: "Guest", enabled: true, state: "stumbling", light: "yellow"}
        const toggle = findChild(row, "pluginEnabledToggle")
        verify(toggle.checked)
        mouseClick(toggle, toggle.width / 2, toggle.height / 2)
        compare(changes.pop(), false)
        row.plugin = {name: "Guest", enabled: false, state: "awaiting approval", light: "yellow"}
        verify(!toggle.checked)
        mouseClick(toggle, toggle.width / 2, toggle.height / 2)
        compare(changes.pop(), true)
    }
    function test_a_plugin_that_cannot_run_here_says_why_and_never_switches_on() {
        const reason = "Process plugins cannot run on this system yet."
        row.plugin = {name: "Process", enabled: false, state: "awaiting approval", light: "native", unavailable: reason}
        const note = findChild(row, "pluginUnavailable")
        verify(note.visible)
        compare(note.text, reason)
        const toggle = findChild(row, "pluginEnabledToggle")
        verify(!toggle.enabled)
        const before = changes.length
        mouseClick(toggle, toggle.width / 2, toggle.height / 2)
        toggle.toggled(true)
        compare(changes.length, before, "it is not switched on, by the pointer or otherwise")
        row.plugin = {name: "Process", enabled: true, state: "unavailable", light: "native", unavailable: reason}
        verify(toggle.enabled, "one left switched on can still be switched off")
        mouseClick(toggle, toggle.width / 2, toggle.height / 2)
        compare(changes.pop(), false)
        row.plugin = {name: "Guest", enabled: true, state: "fine", light: "native"}
        verify(!note.visible)
    }
    function test_installed_plugins_always_offer_details_and_green_is_a_color() {
        row.plugin = {name: "Guest", enabled: true, state: "fine", light: "green"}
        compare(findChild(row, "pluginSettingsButton").enabled, true)
        compare(findChild(row, "pluginCompatibilityLight").color, Theme.color.statusPositive)
    }
}
