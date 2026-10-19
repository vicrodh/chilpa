import QtQuick
import QtTest
import "../../qml/Patterns"
import "../../qml/Primitives"
import "../../qml/Theme"

TestCase {
    name: "PageTransition"
    when: windowShown
    visible: true
    width: 400
    height: 300

    Item { id: outsideFocus }
    Component { id: broken; Item { required property string missingInput } }
    Component { id: albums; Rectangle { objectName: "albumsPage"; color: "red" } }
    Component { id: home; Rectangle { objectName: "homePage"; color: "blue" } }
    // A page that draws only a corner and is transparent everywhere else.
    Component { id: sparse; Item { Rectangle { x: 300; y: 200; width: 50; height: 50; color: "red" } } }
    Component {
        id: delayed
        Rectangle {
            id: delayedPage
            objectName: "delayedPage"
            property bool visualReady: false
            Timer { interval: 1500; running: true; onTriggered: delayedPage.visualReady = true }
        }
    }
    Rectangle {
        id: stage
        anchors.fill: parent
        color: "black"
        PageTransition { id: pages; anchors.fill: parent }
    }
    Component {
        id: clickable
        Rectangle {
            color: "red"
            readonly property string mountedRoute: parent.pageRoute
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    pages.sourceComponent = home
                    pages.route = "from-click"
                }
            }
        }
    }
    Component {
        id: scrolling
        Flickable {
            readonly property string mountedRoute: parent.pageRoute
            contentWidth: width; contentHeight: 1800
        }
    }

    property bool sawAcknowledgement: false
    property bool mountedAfterAcknowledgement: false
    property bool preparedAfterAcknowledgement: false
    property bool redirectPreparation: false
    property bool resolvedSupersededPreparation: false
    property int acknowledgementSkeletonPixel: -1
    property int acknowledgementPlanePixel: -1
    property bool skeletonAtMount: false
    Connections {
        target: pages
        function onPhaseChanged() {
            if (pages.phase === "content")
                skeletonAtMount = findChild(pages, "acknowledgementSkeleton").visible
        }
    }
    Connections {
        target: pages
        function onComponentRouteChanged() {
            if (pages.componentRoute === "prepare-superseded") resolvedSupersededPreparation = true
        }
        function onPreparingRoute() {
            if (redirectPreparation && pages.acknowledgedRoute === "prepare-superseded") {
                redirectPreparation = false
                pages.route = "prepare-final"
            }
            if (pages.acknowledgedRoute === "slow-construction")
                preparedAfterAcknowledgement = sawAcknowledgement && pages.componentRoute !== "slow-construction"
        }
        function onAcknowledgedFrame(destination) {
            if (destination === "slow-construction") {
                sawAcknowledgement = true
                const frame = grabImage(stage)
                acknowledgementSkeletonPixel = frame.red(68, 36)
                acknowledgementPlanePixel = frame.red(5, 5)
            }
        }
    }
    Component {
        id: slowConstruction
        Rectangle {
            Component.onCompleted: {
                mountedAfterAcknowledgement = sawAcknowledgement
                const until = Date.now() + 150
                while (Date.now() < until) {}
            }
        }
    }
    function test_acknowledgement_precedes_expensive_construction() {
        Motion.reduced = true
        pages.sourceComponent = albums
        pages.route = "before-slow-construction"
        tryCompare(pages, "committedRoute", pages.route)
        const previous = pages.item
        sawAcknowledgement = false
        mountedAfterAcknowledgement = false
        preparedAfterAcknowledgement = false
        pages.sourceComponent = slowConstruction
        pages.route = "slow-construction"
        compare(pages.acknowledgedRoute, "slow-construction")
        compare(pages.item, null)
        compare(previous.parent.visible, false)
        compare(previous.parent.enabled, false)
        compare(sawAcknowledgement, false)
        tryCompare(pages, "committedRoute", "slow-construction")
        verify(mountedAfterAcknowledgement, "I present the acknowledgement before construction")
        verify(preparedAfterAcknowledgement, "I prepare after acknowledgement and before component resolution")
        compare(acknowledgementSkeletonPixel, 128, "I paint the destination skeleton before construction")
        compare(acknowledgementPlanePixel, 0, "I leave the plane's own background around the skeleton")
        compare(skeletonAtMount, false, "with reduced motion the skeleton leaves on the frame that shows the page")
        tryCompare(pages, "awaitingContentFrame", false)
    }

    function test_a_cancelled_acknowledgement_cannot_mount_its_route() {
        Motion.reduced = true
        pages.sourceComponent = scrolling
        pages.route = "cancel-origin"
        tryCompare(pages, "committedRoute", pages.route)
        const previous = pages.item
        previous.forceActiveFocus()
        verify(previous.activeFocus)
        pages.route = "cancel-middle"
        const cancelled = pages.generation
        compare(previous.activeFocus, false)
        pages.route = "cancel-final"
        pages.begin(cancelled)
        compare(pages.componentRoute, "cancel-origin")
        compare(pages.item, null)
        tryCompare(pages, "committedRoute", "cancel-final")
        compare(pages.item.mountedRoute, "cancel-final")
    }

    function test_component_invalidation_uses_a_new_acknowledgement() {
        Motion.reduced = true
        pages.sourceComponent = albums
        pages.route = "invalidate-component"
        tryCompare(pages, "committedRoute", pages.route)
        outsideFocus.forceActiveFocus()
        verify(outsideFocus.activeFocus)
        pages.sourceComponent = home
        verify(outsideFocus.activeFocus, "I preserve focus outside the departing page")
        compare(pages.phase, "acknowledging")
        compare(pages.item, null)
        tryCompare(pages, "phase", "content")
        compare(pages.item.objectName, "homePage")
    }

    function test_a_loader_error_can_retry() {
        ignoreWarning(/.*Required property missingInput was not initialized/)
        pages.sourceComponent = broken
        pages.route = "broken-page"
        tryCompare(pages, "phase", "error")
        compare(pages.item, null)
        pages.sourceComponent = home
        pages.request()
        tryCompare(pages, "phase", "content")
        compare(pages.item.objectName, "homePage")
    }

    function test_cancelling_a_loading_page_cannot_present_it() {
        Motion.reduced = true
        pages.sourceComponent = albums
        pages.route = "loading-origin"
        tryCompare(pages, "committedRoute", pages.route)
        pages.route = "loading-superseded"
        pages.awaitingFrame = false
        pages.begin(pages.generation)
        compare(pages.phase, "loading")
        pages.route = "loading-final"
        compare(pages.item, null)
        tryCompare(pages, "committedRoute", "loading-final")
        compare(pages.componentRoute, "loading-final")
    }

    function test_preparation_can_replace_the_destination_before_resolution() {
        Motion.reduced = true
        pages.sourceComponent = home
        pages.route = "prepare-origin"
        tryCompare(pages, "committedRoute", pages.route)
        resolvedSupersededPreparation = false
        redirectPreparation = true
        pages.route = "prepare-superseded"
        tryCompare(pages, "committedRoute", "prepare-final")
        compare(resolvedSupersededPreparation, false)
    }

    function test_a_forced_same_route_request_acknowledges_again() {
        Motion.reduced = true
        pages.sourceComponent = home
        pages.route = "same-route-preparation"
        tryCompare(pages, "committedRoute", pages.route)
        const previousGeneration = pages.generation
        pages.request(true)
        compare(pages.generation, previousGeneration + 1)
        compare(pages.phase, "acknowledging")
        compare(pages.item, null)
        tryCompare(pages, "phase", "content")
        compare(pages.committedRoute, "same-route-preparation")
    }

    function cleanup() {
        Motion.reduced = true
        pages.route = ""
        pages.sourceComponent = null
        pages.start()
        Motion.reduced = false
    }

    function test_reveal_accepts_input_and_the_next_route_without_waiting() {
        pages.sourceComponent = clickable
        pages.route = "click-during-reveal"
        tryCompare(pages, "committedRoute", pages.route)
        verify(pages.item.parent.opacity < 1, "the page is still arriving")
        mouseClick(pages.item, 100, 100)
        tryCompare(pages, "committedRoute", "from-click", 200)
        compare(pages.item.objectName, "homePage")
        verify(pages.item.parent.opacity < 1, "the next page has its own reveal")
        tryCompare(pages.item.parent, "opacity", 1)
    }

    // The software renderer keeps an eased reveal's last step when it is
    // within float tolerance of 1 (QSGSoftwareRenderableNode::setOpacity
    // compares with qFuzzyCompare), so a settled page reads 254 or 255 here.
    // The scene graph's own opacity node compares exactly.
    function settled(red) { return red >= 254 }

    function test_reveal_changes_pixels_and_settles() {
        pages.sourceComponent = albums
        pages.route = "visible-reveal"
        tryCompare(pages, "committedRoute", pages.route)
        const arriving = grabImage(stage).red(100, 100)
        verify(arriving < 255, "the render thread reveals the page: " + arriving)
        tryCompare(pages.item.parent, "opacity", 1)
        tryVerify(() => settled(grabImage(stage).red(100, 100)), 1000, "the page ends opaque")
    }

    function test_the_skeleton_dissolves_into_the_mounted_page() {
        // A page is transparent wherever it draws nothing, so the skeleton
        // under it shows there until it is gone. It starts leaving with the
        // mount and fades out, never staying whole behind the arriving page
        // and leaving it in one step.
        pages.sourceComponent = sparse
        pages.route = "skeleton-dissolves"
        const skeleton = findChild(pages, "acknowledgementSkeleton")
        tryCompare(pages, "committedRoute", pages.route)
        verify(skeletonAtMount, "the skeleton is still there on the frame the page mounts")
        const bar = []
        for (let shot = 0; shot < 60 && skeleton.visible; shot++) {
            bar.push(grabImage(stage).red(68, 36))
            wait(5)
        }
        verify(!skeleton.visible, "the skeleton leaves")
        verify(pages.item.parent.opacity < 1, "the skeleton is gone before the page has finished arriving")
        verify(bar.some(red => red > 0 && red < 128), "the skeleton fades instead of leaving in one step: " + bar)
        compare(grabImage(stage).red(68, 36), 0, "nothing of the skeleton stays under the page")
        tryCompare(pages.item.parent, "opacity", 1)
        tryVerify(() => settled(grabImage(stage).red(320, 220)), 1000, "the page ends opaque")
    }

    function test_rapid_routes_replace_the_reveal_and_restore_scroll() {
        pages.sourceComponent = scrolling
        pages.route = "rapid-first"
        tryCompare(pages, "committedRoute", pages.route)
        pages.item.contentY = 320
        pages.route = "rapid-superseded"
        pages.start()
        pages.route = "rapid-last"
        pages.start()
        tryCompare(pages, "committedRoute", "rapid-last", 200)
        compare(pages.item.mountedRoute, "rapid-last")
        compare(pages.item.contentY, 0)
        pages.route = "rapid-first"
        tryCompare(pages, "committedRoute", "rapid-first", 200)
        compare(pages.item.mountedRoute, "rapid-first")
        compare(pages.item.contentY, 320)
        tryCompare(pages.item.parent, "opacity", 1)
    }

    function test_reduced_motion_skips_and_cancels_reveal() {
        Motion.reduced = true
        pages.sourceComponent = albums
        pages.route = "reduced"
        tryCompare(pages, "committedRoute", pages.route)
        compare(pages.item.parent.opacity, 1)
        compare(grabImage(stage).red(100, 100), 255)

        Motion.reduced = false
        pages.route = "reduced-during-reveal"
        tryCompare(pages, "committedRoute", pages.route)
        verify(pages.item.parent.opacity < 1)
        Motion.reduced = true
        compare(pages.item.parent.opacity, 1)
        wait(350)
        compare(pages.item.parent.opacity, 1, "a stopped animator cannot hide the page again")
    }

    function test_pages_keep_their_own_route_and_scroll() {
        pages.sourceComponent = scrolling
        pages.route = "first-scrolling-page"
        tryCompare(pages, "committedRoute", pages.route)
        pages.item.contentY = 420
        const previous = pages.item
        pages.route = "second-scrolling-page"
        compare(previous.mountedRoute, "first-scrolling-page",
            "preparing the next page must not retarget the visible one")
        compare(previous.contentY, 420)
        tryCompare(pages, "committedRoute", pages.route)
        compare(pages.item.contentY, 0)
        compare(Visit.recall("first-scrolling-page", -1), 420)
        pages.item.contentY = 210
        pages.route = "first-scrolling-page"
        tryCompare(pages, "committedRoute", pages.route)
        compare(pages.item.contentY, 420)
        compare(Visit.recall("second-scrolling-page", -1), 210)
    }

    function test_a_slow_module_does_not_delay_its_destination_shell() {
        pages.sourceComponent = albums
        pages.route = "albums"
        tryCompare(pages, "committedRoute", "albums")
        compare(pages.item.objectName, "albumsPage")

        pages.sourceComponent = delayed
        pages.route = "delayed"
        const started = Date.now()
        tryCompare(pages, "committedRoute", "delayed", 500)
        compare(pages.item.objectName, "delayedPage")
        verify(Date.now() - started < 500)
        compare(pages.item.visualReady, false, "the page owns its skeleton while data is still pending")

        pages.sourceComponent = home
        pages.route = "home"
        compare(pages.item, null, "I hide the old page immediately")
        tryCompare(pages, "committedRoute", "home")
        compare(pages.item.objectName, "homePage")

        pages.sourceComponent = albums
        pages.route = "albums"
        compare(pages.item, null, "I acknowledge the return trip immediately")
        tryCompare(pages, "committedRoute", "albums")
        compare(pages.item.objectName, "albumsPage")
    }
}
