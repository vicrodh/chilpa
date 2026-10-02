import QtQuick
import QtTest
import "../../qml/Components"
import "../../qml/Theme"

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
    function test_missing_settings_are_disabled_and_green_is_a_color() {
        row.plugin = {name: "Guest", enabled: true, state: "fine", light: "green"}
        compare(findChild(row, "pluginSettingsButton").enabled, false)
        compare(findChild(row, "pluginCompatibilityLight").color, Theme.color.statusPositive)
    }
}
