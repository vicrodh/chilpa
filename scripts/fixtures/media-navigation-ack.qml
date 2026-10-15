import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property string pluginId: "app.test.spoken"
    property bool measuring: false
    property double clickedAt: -1
    property double acknowledgedAt: -1
    property double preparedAt: -1
    property bool hiddenAtAcknowledgement: false
    property bool previousParentAtAcknowledgement: false
    property string previousParent: ""
    property var previousPage: null
    property string expectedParent: ""
    property int requestedSignals: 0
    readonly property var snapshot: JSON.parse(Sources.snapshot || "{}")
    ScreenHost { id: host; anchors.fill: parent }
    SearchModel { id: search; scope: "albums"; source: window.pluginId }
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
    Connections {
        target: Navigation
        function onRouteRequested() { if (window.measuring) window.requestedSignals++ }
    }
    Connections {
        target: host
        function onAcknowledgedFrame(route) {
            if (!window.measuring) return
            window.acknowledgedAt = Date.now()
            window.hiddenAtAcknowledgement = !host.item && !window.previousPage.parent.visible && !window.previousPage.parent.enabled
            window.previousParentAtAcknowledgement = (window.snapshot.about?.title || "") === window.previousParent
        }
        function onPreparingRoute() {
            if (window.measuring) window.preparedAt = Date.now()
        }
    }
    TestCase {
        name: "NativeSameRouteMediaAcknowledgement"
        when: window.visible
        function check(value, message) {
            if (!value) console.log("SMOKE FAIL " + message)
            verify(value, message)
        }
        function test_two_real_show_clicks_prepare_the_same_source_route() {
            tryVerify(() => (window.snapshot.panels || []).some(panel => panel.id === window.pluginId), 15000)
            Sources.command("open", window.pluginId, "")
            tryVerify(function() {
                Sources.refresh()
                return window.snapshot.medium === "podcasts" && !window.snapshot.loading
            }, 10000)
            Navigation.go("library", 0)
            tryVerify(() => host.phase === "content" && Navigation.screen === "library", 10000)
            const shows = [{id:"show first /?#", title:"First opaque show"}, {id:"show second /?#", title:"Second opaque show"}]
            let previousRoute = ""
            for (let index = 0; index < shows.length; index++) {
                const show = shows[index]
                window.previousPage = host.item
                window.previousParent = window.snapshot.about?.title || ""
                window.expectedParent = show.id
                window.acknowledgedAt = -1
                window.preparedAt = -1
                window.hiddenAtAcknowledgement = false
                window.previousParentAtAcknowledgement = false
                window.requestedSignals = 0
                search.source = window.pluginId
                search.text = show.title
                results.open()
                tryCompare(results, "opened", true)
                let hit = null
                tryVerify(function() {
                    hit = window.find(results.contentItem, item => item.kind === 3 && item.title === show.title)
                    return hit !== null && hit.visible && hit.width > 0
                }, 10000)
                waitForRendering(hit)
                const generation = host.generation
                window.measuring = true
                window.clickedAt = Date.now()
                mouseClick(hit, hit.width * 0.3, hit.height * 0.5, Qt.LeftButton, Qt.NoModifier, 0)
                tryVerify(() => window.acknowledgedAt >= 0 && host.phase === "content" && window.snapshot.about?.title === show.title && !window.snapshot.loading, 10000)
                window.measuring = false
                check(host.generation > generation, "I acknowledge each selected media parent")
                check(window.requestedSignals === 1, "I request media preparation once per click")
                check(window.hiddenAtAcknowledgement, "I show the shell and disable the old media page")
                check(window.previousParentAtAcknowledgement, "I retain the old parent until its acknowledgement frame")
                check(window.preparedAt >= window.acknowledgedAt, "I prepare media only after acknowledgement")
                check(Navigation.screen === "source" && Navigation.routeKey === window.pluginId, "I keep the source route")
                check(window.snapshot.medium === "podcasts" && !window.snapshot.music, "I retain spoken media classification")
                check(typeof host.item.episodeFilter === "string", "I mount the spoken media page rather than a music shelf")
                check(window.snapshot.about.title === show.title, "I publish the selected show's header")
                check(window.find(host.item, item => item.text === show.title) !== null, "I render the selected show's heading")
                const crumbs = window.snapshot.crumbs || []
                check(crumbs.length === 1 && crumbs[0] === show.title, "I replace the trail with the selected opaque parent")
                check((window.snapshot.entries || []).some(entry => entry.id === show.id + "/episode" && entry.title === show.title + " episode"), "I load the selected show's episode listing")
                check(!Player.playing || Player.paused, "I keep navigation silent")
                if (index) check(host.committedRoute === previousRoute, "I prepare a different parent without changing the route")
                previousRoute = host.committedRoute
                console.log("SMOKE media acknowledgement " + JSON.stringify({sample:index, parent:show.id,
                    acknowledgement_ms:window.acknowledgedAt - window.clickedAt,
                    preparation_ms:window.preparedAt - window.clickedAt, same_route:index === 1,
                    old_hidden:window.hiddenAtAcknowledgement, parent_unchanged_before_ack:window.previousParentAtAcknowledgement,
                    about:window.snapshot.about, crumbs:crumbs}))
            }
        }
        function cleanupTestCase() {
            if (qtest_results.failCount) { console.log("SMOKE FAIL media assertions=" + qtest_results.failCount); Qt.exit(1) }
            else { console.log("SMOKE media navigation acknowledgement completed"); Qt.exit(0) }
        }
    }
    Timer { interval: 45000; running: true; onTriggered: { console.log("SMOKE FAIL media navigation timed out"); Qt.exit(1) } }
}
