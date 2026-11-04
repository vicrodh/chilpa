import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Components" as Presentation

// The approved mount lives with its page: a plugin surface without the
// display contract goes while the page is hidden and comes back with it;
// a display that took the window keeps its surface, and a new revision of
// it loads in place.
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
    Item { id: second; width: 900; height: 700; property string pluginId: "second" }
    Presentation.PluginViewHost { id: host; width: 900; height: 700; window: owner }
    readonly property var mode: Presentation.DisplayMode
    readonly property string plain: "qrc:/qt/qml/app/chilpa/kit/qml/Primitives/Edge.qml"
    readonly property string display: Qt.resolvedUrl("fixtures/DisplaySurface.qml")
    function offer(id, url) { const next = Object.assign({}, Sources.displays); next[id] = url; Sources.displays = next; Sources.revision++ }
    function init() { page.visible = true; second.visible = true }
    function cleanup() { mode.leave(); mode.unmount(second); mode.unmount(page); Sources.displays = {} }
    // The window is the display's, so the page under it is hidden.
    function holdTheWindow(mount) {
        offer("fixture", display + "?revision=1")
        mode.mount(page)
        tryVerify(() => mount.item !== null && mode.active)
        page.visible = false
    }
    function test_a_plain_surface_goes_while_its_page_is_hidden() {
        const mount = findChild(host, "pluginDisplayLoader")
        offer("fixture", plain)
        mode.mount(page)
        tryVerify(() => mount.item !== null)
        verify(!mode.active)
        page.visible = false
        tryVerify(() => mount.item === null)
        page.visible = true
        tryVerify(() => mount.item !== null)
    }
    function test_a_display_keeps_its_surface_while_it_holds_the_window() {
        const mount = findChild(host, "pluginDisplayLoader")
        holdTheWindow(mount)
        wait(0)
        verify(mount.item !== null)
        compare(mode.surface, mount.item)
    }
    function test_a_new_revision_of_a_display_loads_in_place() {
        const mount = findChild(host, "pluginDisplayLoader")
        holdTheWindow(mount)
        const old = mount.item
        let dropped = 0
        const watch = () => { if (!mount.active) dropped++ }
        mount.activeChanged.connect(watch)
        offer("fixture", display + "?revision=2")
        tryVerify(() => mount.item !== null && mount.item !== old && mode.surface === mount.item)
        mount.activeChanged.disconnect(watch)
        compare(dropped, 0)
        compare(String(mount.source), display + "?revision=2")
        verify(mode.active)
    }
    function test_another_display_asked_while_one_holds_the_window_loads() {
        const mount = findChild(host, "pluginDisplayLoader")
        holdTheWindow(mount)
        const old = mount.item
        offer("second", display + "?revision=3")
        second.visible = false
        mode.mount(second)
        tryVerify(() => mount.item !== null && mount.item !== old && mode.pluginId === "second" && mode.surface === mount.item)
        compare(String(mount.source), display + "?revision=3")
        verify(mode.active)
    }
}
