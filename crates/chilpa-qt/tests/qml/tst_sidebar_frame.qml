import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml"

// The sidebar's grip: a drag follows the pointer, a mode it snaps to
// slides there as QBZ's does instead of jumping, and the release keeps
// the mode it showed.
TestCase {
    name: "SidebarFrame"
    when: windowShown
    visible: true
    width: 400
    height: 300

    Window {
        id: win
        width: 1280
        height: 800
        visible: true
        SidebarFrame { id: frame; window: win; anchors.fill: parent }
    }

    function handle() {
        return frame.children.find(child => child.moved !== undefined && child.released !== undefined)
    }

    function test_a_snap_during_a_drag_slides_and_the_pointer_is_followed_otherwise() {
        SettingsModel.sidebarMode = "open"
        const grip = handle()
        grip.started()
        grip.moved(40)
        verify(!frame.snapping, "open to open follows the pointer")
        compare(frame.shown, Metrics.sidebarWidth + 40)
        grip.moved(-100)
        verify(frame.snapping, "a new mode slides")
        verify(frame.shown > Metrics.sidebarCollapsed, "on its way, not already there: " + frame.shown)
        tryCompare(frame, "shown", Metrics.sidebarCollapsed)
        grip.released()
        compare(SettingsModel.placed[0], "collapsed", "it stays as it showed")
        verify(!frame.snapping)
        SettingsModel.sidebarMode = "expanded"
    }
}
