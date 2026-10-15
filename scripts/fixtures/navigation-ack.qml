import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/qml"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property double fixtureAlbum: "FIXTURE_ALBUM"
    property string fixtureTitle: "FIXTURE_TITLE"
    property int fixtureTracks: "FIXTURE_TRACKS"
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property double injectedAt: -1
    property double activatedAt: -1
    property double logicalAt: -1
    property double acknowledgedAt: -1
    property double resolvedAt: -1
    property double mountedAt: -1
    property double contentAt: -1
    property var oldPage: null
    property bool oldHidden: false
    property bool resolvedAfterFrame: false
    property bool activatedBeforeNavigation: false
    property int visibleRows: 0
    property int tracks: 0
    ScreenHost { id: host; anchors.fill: parent }
    SearchModel { id: search; scope: "albums"; source: "local" }
    SearchFlyout { id: results; search: search; room: 700; x: 100; y: 30 }

    function find(item, predicate) {
        if (!item) return null
        if (predicate(item)) return item
        for (const child of item.children || []) {
            const found = find(child, predicate)
            if (found) return found
        }
        return null
    }
    function destination() { return Navigation.screen === "album" && Navigation.routeId === fixtureAlbum }
    Connections {
        target: host
        function onAcknowledgedRouteChanged() {
            if (window.activatedAt >= 0 && window.destination()) {
                window.logicalAt = Date.now()
                window.oldHidden = !host.item && window.oldPage && !window.oldPage.parent.visible && !window.oldPage.parent.enabled
            }
        }
        function onPhaseChanged() {
            if (window.activatedAt >= 0 && window.destination() && host.phase === "acknowledging")
                window.oldHidden = !host.item && window.oldPage && !window.oldPage.parent.visible && !window.oldPage.parent.enabled
        }
        function onAcknowledgedFrame(route) {
            if (window.activatedAt >= 0 && window.destination()) {
                window.acknowledgedAt = Date.now()
                window.oldHidden = !host.item && window.oldPage && !window.oldPage.parent.visible && !window.oldPage.parent.enabled
            }
        }
        function onComponentRouteChanged() {
            if (window.activatedAt >= 0 && window.destination()) {
                window.resolvedAt = Date.now()
                window.resolvedAfterFrame = window.acknowledgedAt >= 0
            }
        }
        function onItemChanged() {
            if (window.activatedAt >= 0 && window.destination() && host.item) window.mountedAt = Date.now()
        }
    }
    Connections {
        target: window
        function onFrameSwapped() {
            if (window.contentAt >= 0 || window.activatedAt < 0 || !window.destination() || !host.item) return
            const view = window.find(host.item, item => item.objectName === "trackListView")
            if (!view || !view.model.ready || !view.model.count) return
            let rows = 0
            for (let index = 0; index < view.count; index++)
                if (window.find(view.itemAtIndex(index), item => item.objectName === "trackRowBody")) rows++
            if (!rows) return
            window.visibleRows = rows
            window.tracks = view.model.count
            window.contentAt = Date.now()
        }
    }
    TestCase {
        id: tests
        name: "NativeNavigationAcknowledgement"
        when: window.visible
        function check(value, message) {
            if (!value) console.log("SMOKE FAIL " + message)
            verify(value, message)
        }
        function structure(count) {
            check(Navigation.tabTitles.length === count, "I publish the current number of titles")
            check(Navigation.tabIcons.length === count, "I publish the current number of icons")
            check(Navigation.active >= 0 && Navigation.active < count, "I keep the active tab in range")
        }
        function test_actual_search_click_acknowledges_before_mount() {
            Navigation.go("library", 0)
            tryVerify(() => host.phase === "content" && Navigation.screen === "library", 10000)
            window.oldPage = host.item
            search.source = "local"
            search.text = window.fixtureTitle
            results.open()
            tryCompare(results, "opened", true)
            let hit = null
            tryVerify(function() {
                hit = window.find(results.contentItem, item => item.kind === 3 && item.title === window.fixtureTitle)
                return hit !== null && hit.visible && hit.width > 0
            }, 10000)
            waitForRendering(hit)
            hit.activated.connect(function() {
                window.activatedAt = Date.now()
                window.activatedBeforeNavigation = !window.destination()
            })
            window.injectedAt = Date.now()
            mouseClick(hit, hit.width * 0.3, hit.height * 0.5, Qt.LeftButton, Qt.NoModifier, 0)
            check(window.activatedAt >= 0, "I activate the real search row with a mouse click")
            check(window.activatedBeforeNavigation, "I timestamp activation before deferred navigation")
            tryVerify(() => window.contentAt >= 0, 15000)
            check(window.oldHidden, "I hide and disable the previous page at acknowledgement")
            check(window.logicalAt >= window.activatedAt, "I observe the logical acknowledgement")
            check(window.acknowledgedAt >= window.logicalAt, "I present the acknowledgement frame")
            check(window.resolvedAfterFrame, "I resolve the component after the acknowledgement frame")
            check(window.mountedAt >= window.acknowledgedAt, "I mount after acknowledgement")
            check(window.contentAt >= window.mountedAt, "I measure rows separately from mounting")
            check(window.logicalAt - window.injectedAt <= 16, "I acknowledge the injected click logically within 16 ms")
            check(window.acknowledgedAt - window.injectedAt < 150, "I present the click acknowledgement within 150 ms")
            check(window.tracks >= 100 && window.visibleRows > 0, "I present the large record's real rows")
            console.log("SMOKE navigation measurements " + JSON.stringify({
                injected: window.injectedAt, activated: window.activatedAt,
                click_to_activation_ms: window.activatedAt - window.injectedAt,
                click_to_logical_ms: window.logicalAt - window.injectedAt,
                activation_to_logical_ms: window.logicalAt - window.activatedAt,
                click_to_acknowledged_frame_ms: window.acknowledgedAt - window.injectedAt,
                acknowledged_frame_ms: window.acknowledgedAt - window.activatedAt,
                component_resolution_ms: window.resolvedAt - window.activatedAt,
                mounted_ms: window.mountedAt - window.activatedAt,
                first_rows_frame_ms: window.contentAt - window.activatedAt,
                rows: window.visibleRows, tracks: window.tracks, old_hidden: window.oldHidden,
                resolved_after_acknowledgement: window.resolvedAfterFrame
            }))
            // I capture only after timing ends so readback cannot delay acknowledgement.
            host.grabToImage(result => result.saveToFile(window.outputDirectory + "/content.png"))
        }
        function test_pending_settlement_preserves_the_current_structure() {
            const original = Navigation.tabTitles.length
            Navigation.open("home", 0)
            const stale = Navigation.pendingRevision
            Navigation.go("library", 0)
            const current = Navigation.pendingRevision
            check(current !== stale, "I advance the settlement revision")
            Navigation.settle(stale)
            check(Navigation.pending && Navigation.pendingRevision === current, "I reject stale settlement")
            structure(original + 1)
            tryCompare(Navigation, "pending", false)
            const beforeClose = Navigation.screen
            Navigation.close(0)
            check(Navigation.screen === beforeClose, "I retain the route when closing an inactive tab")
            check(Navigation.pending, "I defer inactive-tab persistence")
            structure(original)
            tryCompare(Navigation, "pending", false)
            Navigation.go("library", 0)
            check(Navigation.pending, "I settle publication even when the route stays unchanged")
            tryCompare(Navigation, "pending", false)
            structure(original)
            for (let index = 0; index < Navigation.tabTitles.length; index++) {
                check(Navigation.tabTitles[index] !== "" && Navigation.tabTitles[index] !== "…", "I enrich current tab titles")
                check(Navigation.tabIcons[index] !== "" && Navigation.tabIcons[index] !== "dots", "I enrich current tab icons")
            }
            check(Navigation.screen === "library" && Navigation.routeId === 0, "I never publish an obsolete destination")
        }
        function cleanupTestCase() {
            if (qtest_results.failCount > 0) {
                console.log("SMOKE FAIL navigation assertions=" + qtest_results.failCount)
                Qt.exit(1)
            } else {
                console.log("SMOKE navigation acknowledgement completed")
                Qt.exit(0)
            }
        }
    }
    Timer { interval: 40000; running: true; onTriggered: { console.log("SMOKE FAIL navigation acknowledgement timed out"); Qt.exit(1) } }
}
