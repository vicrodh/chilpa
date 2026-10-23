import QtQuick
import QtTest
import app.chilpa.about
import "../../qml/Components"

TestCase {
    name: "PluginUpdates"
    when: windowShown
    visible: true
    width: 960; height: 720
    UpdatesIndicator { id: indicator }
    PluginUpdatesPanel { id: panel; y: 40; width: 800 }
    function init() { PluginUpdates.calls = []; PluginUpdates.snapshot = "{}"; Updates.snapshot = "{}" }
    function test_one_indicator_covers_app_and_plugin_updates() {
        compare(indicator.unread, false)
        Updates.snapshot = JSON.stringify({supported:true,phase:"available",version:"1.2.0"})
        compare(indicator.unread, true)
        PluginUpdates.snapshot = JSON.stringify({seen_app:"1.2.0:available",unread:0})
        compare(indicator.unread, false)
        PluginUpdates.snapshot = JSON.stringify({seen_app:"1.2.0:available",unread:1})
        compare(indicator.unread, true)
        Updates.snapshot = JSON.stringify({supported:true,phase:"current"})
        compare(indicator.unread, true)
    }
    function test_check_apply_and_automatic_authorization_are_separate_actions() {
        mouseClick(findChild(panel,"checkPluginUpdates"))
        compare(PluginUpdates.calls[0], ["updates_check",{}])
        verify(!findChild(panel,"applyPluginUpdates").enabled)
        PluginUpdates.snapshot = JSON.stringify({automatic:false,items:[{name:"One",version:"2.0.0",phase:"available",message:""}]})
        mouseClick(findChild(panel,"applyPluginUpdates"))
        compare(PluginUpdates.calls[1], ["updates_apply",{}])
        const toggle = findChild(panel,"automaticPluginUpdates")
        verify(!toggle.checked)
        toggle.toggled(true)
        compare(PluginUpdates.calls[2], ["updates_automatic",{enabled:true}])
    }
    function test_a_new_plugin_is_announced_and_leads_to_the_plugin_list() {
        let reviewed = 0
        panel.review = function() { reviewed += 1 }
        PluginUpdates.snapshot = JSON.stringify({items:[
            {name:"Fresh",version:"1.0.0",phase:"new",third_party:false,message:""},
            {name:"Outside",version:"0.3.0",phase:"new",third_party:true,message:""}]})
        const list = findChild(panel,"pluginUpdateInbox")
        tryCompare(list,"count",2)
        verify(!findChild(panel,"applyPluginUpdates").enabled, "a new plugin is not an update")
        tryVerify(() => list.itemAtIndex(1) !== null, 2000, "both rows are built")
        const first = list.itemAtIndex(0), second = list.itemAtIndex(1)
        compare(findChild(first,"pluginNoticeStatus").text, "New plugin")
        compare(findChild(second,"pluginNoticeStatus").text, "New third-party plugin")
        const open = findChild(first,"openNewPlugin")
        verify(open.visible)
        mouseClick(open)
        compare(reviewed, 1)
        panel.review = function() {}
    }
    function test_large_inbox_uses_a_bounded_viewport() {
        const items = Array.from({length:128}, (_, i) => ({name:"Plugin " + i,version:"2.0.0",phase:"available",message:""}))
        PluginUpdates.snapshot = JSON.stringify({items:items})
        const list = findChild(panel,"pluginUpdateInbox")
        tryCompare(list,"count",128)
        waitForRendering(list)
        verify(list.contentHeight > list.height)
        verify(list.contentItem.children.length < 40)
    }
}
