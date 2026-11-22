import QtQuick
import QtTest
import "../../qml/Patterns"

TestCase {
    id: test
    name: "PluginBrowserSetup"
    when: windowShown
    visible: true
    width: 600
    height: 500
    PluginSettingsForm { id: form; width: 400; document: ({}) }
    SignalSpy { id: refreshed; target: form; signalName: "refreshed" }
    SignalSpy { id: submitted; target: form; signalName: "submitted" }
    function init() { form.visible = true; form.hostBusy = false; refreshed.clear(); submitted.clear() }
    function cleanup() { form.document = ({}); form.visible = false }

    function test_picker_value_is_submitted_without_an_editable_text_field() {
        form.document = {revision:1, form:{fields:[{key:"browser",label:"Browser",control:{type:"path"}}],
            actions:[{key:"next",label:"Next",fields:["browser"]}]}}
        tryVerify(() => findChild(form, "pluginPath_browser") !== null)
        const picker = findChild(form, "pluginPath_browser")
        verify(findChild(form, "pluginField_browser") === null)
        picker.picked("file:///Applications/Example%20Browser.app")
        compare(form.values.browser, "file:///Applications/Example%20Browser.app")
        wait(50) // Let the selected path update the column layout before clicking.
        mouseClick(findChild(form, "pluginAction_next"))
        compare(submitted.count, 1)
        compare(submitted.signalArguments[0][1].browser, "file:///Applications/Example%20Browser.app")
    }
    function test_login_refresh_stops_when_hidden_busy_or_finished() {
        form.document = {wizard:{waiting:true,refresh_ms:1000}}
        tryCompare(refreshed, "count", 1, 1800)
        form.visible = false
        wait(1100)
        compare(refreshed.count, 1)
        form.visible = true
        form.hostBusy = true
        wait(1100)
        compare(refreshed.count, 1)
        form.hostBusy = false
        form.document = {wizard:{waiting:false,refresh_ms:0}}
        wait(1100)
        compare(refreshed.count, 1)
    }
}
