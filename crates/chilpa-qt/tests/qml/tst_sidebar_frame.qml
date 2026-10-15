import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
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

    QtObject { id: router; property int generation: 0; property string phase: "content"; property string route: "source-route"; signal preparingRoute(); function request(force) { generation++ } }

    Window {
        id: win
        width: 1280
        height: 800
        visible: true
        SidebarFrame { id: frame; window: win; router: router; anchors.fill: parent }
    }

    function test_deferred_navigation_waits_for_acknowledgement_and_cancels_old_work() {
        let calls = 0
        frame.deferNavigation(function() { calls += 1 })
        compare(calls, 0)
        router.generation++
        router.preparingRoute()
        compare(calls, 0)
        frame.deferNavigation(function() { calls += 1 })
        router.preparingRoute()
        compare(calls, 1)
        router.preparingRoute()
        compare(calls, 1, "I consume deferred navigation before invoking its callback")
        frame.deferNavigation(function() {
            calls += 1
            frame.deferNavigation(function() { calls += 10 })
        })
        router.preparingRoute()
        compare(calls, 2)
        router.preparingRoute()
        compare(calls, 12, "I preserve work requested by the callback I am consuming")
    }

    function test_source_refreshes_coalesce_behind_the_acknowledgement() {
        const side = frame.children.find(child => typeof child.refreshSources === "function")
        Sources.refreshCount = 0
        router.phase = "acknowledging"
        side.refreshSources()
        side.refreshSources()
        compare(Sources.refreshCount, 0)
        verify(side.refreshPending)
        router.phase = "resolving"
        router.preparingRoute()
        compare(Sources.refreshCount, 1)
        compare(side.refreshPending, false)
        router.phase = "content"
    }

    function test_source_commands_wait_for_preparation_and_cancel_with_the_route() {
        Sources.asked = []
        frame.openSource("child", "fixture", "first")
        compare(Sources.asked.length, 0, "I acknowledge before I ask the source")
        router.generation++
        router.preparingRoute()
        compare(Sources.asked.length, 0, "I discard a superseded source command")
        frame.openSource("home", "fixture", "")
        compare(Sources.asked.length, 0)
        router.preparingRoute()
        compare(Sources.asked, [["home", "fixture", ""]])
        router.preparingRoute()
        compare(Sources.asked.length, 1, "I consume the command before applying it")
        frame.openSource("child", "fixture", "second")
        router.route = "another-route"
        router.preparingRoute()
        compare(Sources.asked.length, 1, "I match the acknowledged destination as well as its generation")
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
