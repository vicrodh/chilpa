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

    Component { id: albums; Rectangle { objectName: "albumsPage"; color: "red" } }
    Component { id: home; Rectangle { objectName: "homePage"; color: "blue" } }
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

    function test_reveal_changes_pixels_and_settles() {
        pages.sourceComponent = albums
        pages.route = "visible-reveal"
        tryCompare(pages, "committedRoute", pages.route)
        const arriving = grabImage(stage).red(100, 100)
        verify(arriving < 255, "the render thread reveals the page: " + arriving)
        tryCompare(pages.item.parent, "opacity", 1)
        compare(grabImage(stage).red(100, 100), 255)
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
        compare(pages.item.objectName, "delayedPage", "the old page stays mounted while Home is prepared")
        tryCompare(pages, "committedRoute", "home")
        compare(pages.item.objectName, "homePage")

        pages.sourceComponent = albums
        pages.route = "albums"
        compare(pages.item.objectName, "homePage", "the old page stays mounted on the return trip")
        tryCompare(pages, "committedRoute", "albums")
        compare(pages.item.objectName, "albumsPage")
    }
}
