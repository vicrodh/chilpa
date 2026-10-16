import QtQuick
import QtTest
import "../../qml/Components"

TestCase {
    name: "FrameSettlement"
    when: windowShown
    visible: true
    width: 120; height: 100
    QtObject {
        id: navigation
        property bool pending: false
        property int pendingRevision: 0
        property int calls: 0
        property int settledRevision: -1
        function publish() { pending = true; pendingRevision++ }
        function settle(revision) {
            if (!pending || revision !== pendingRevision) return
            pending = false
            calls++
            settledRevision = revision
        }
    }
    QtObject {
        id: router
        property string phase: "content"
        signal preparingRoute()
    }
    FrameSettlement { id: settlement; navigation: navigation; router: router }
    function init() { navigation.pending = false; navigation.calls = 0; router.phase = "content" }

    function test_a_ready_frame_waits_for_preparation_and_rechecks_new_acknowledgements() {
        router.phase = "acknowledging"
        navigation.publish()
        tryCompare(settlement, "readyRevision", navigation.pendingRevision)
        compare(navigation.calls, 0)
        router.phase = "content"
        settlement.settle()
        router.phase = "acknowledging"
        wait(30)
        compare(navigation.calls, 0, "I recheck the phase when deferred settlement executes")
        verify(navigation.pending)
        router.phase = "resolving"
        router.preparingRoute()
        tryCompare(navigation, "pending", false)
        compare(navigation.calls, 1)
    }
    function test_preparation_before_frame_delivery_still_settles_the_publication() {
        router.phase = "resolving"
        navigation.publish()
        router.preparingRoute()
        tryCompare(navigation, "pending", false)
        compare(navigation.calls, 1)
        compare(navigation.settledRevision, navigation.pendingRevision)
    }
    function test_unchanged_routes_and_superseded_revisions_settle_only_the_latest() {
        navigation.publish()
        navigation.publish()
        tryCompare(navigation, "pending", false)
        compare(navigation.calls, 1)
        compare(navigation.settledRevision, navigation.pendingRevision)
    }
}
