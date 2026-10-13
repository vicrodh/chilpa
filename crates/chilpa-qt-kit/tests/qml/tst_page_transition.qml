import QtQuick
import QtTest
import "../../qml/Patterns"

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
