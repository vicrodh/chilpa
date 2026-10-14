import QtQuick
import QtTest
import "../../qml/Patterns"
import "../../qml/Primitives"

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
    PageTransition { id: pages; anchors.fill: parent }
    Component {
        id: scrolling
        Flickable {
            readonly property string mountedRoute: parent.pageRoute
            contentWidth: width; contentHeight: 1800
        }
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
