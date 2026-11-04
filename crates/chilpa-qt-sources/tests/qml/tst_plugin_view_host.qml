import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Components" as Presentation

// The approved mount lives with its page: a plugin surface without the
// display contract goes while the page is hidden and comes back with it;
// a display that took the window keeps its surface.
TestCase {
    id: test
    name: "PluginViewHost"
    when: windowShown
    visible: true; width: 900; height: 700
    Item {
        id: owner
        width: 900; height: 700
        property int visibility: Window.Windowed
        property var contentItem: owner
    }
    Item { id: page; width: 900; height: 700; property string pluginId: "fixture" }
    Item {
        id: display
        property var display: QtObject {
            property int version: 1
            property var host: null
            property bool useCustomPlayer: false
            property Component playerComponent: null
            property var menuEntries: []
            function command(action) {}
        }
    }
    Presentation.PluginViewHost { id: host; width: 900; height: 700; window: owner }
    readonly property var mode: Presentation.DisplayMode
    function init() {
        Sources.displays = {fixture: "qrc:/qt/qml/app/chilpa/kit/qml/Primitives/Edge.qml"}
        page.visible = true
        mode.mount(page)
    }
    function cleanup() { mode.unmount(page); Sources.displays = {} }
    function test_a_plain_surface_goes_while_its_page_is_hidden() {
        const mount = findChild(host, "pluginDisplayLoader")
        tryVerify(() => mount.item !== null)
        verify(!mode.active)
        page.visible = false
        tryVerify(() => mount.item === null)
        page.visible = true
        tryVerify(() => mount.item !== null)
    }
    function test_a_display_keeps_its_surface_while_it_holds_the_window() {
        const mount = findChild(host, "pluginDisplayLoader")
        tryVerify(() => mount.item !== null)
        mode.bind(display)
        verify(mode.active)
        page.visible = false
        wait(0)
        verify(mount.item !== null)
        mode.leave()
    }
}
