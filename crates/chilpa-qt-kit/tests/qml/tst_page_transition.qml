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
            Timer { interval: 100; running: true; onTriggered: delayedPage.visualReady = true }
        }
    }
    PageTransition { id: pages; anchors.fill: parent }

    function test_current_page_remains_until_the_destination_is_ready() {
        pages.sourceComponent = albums
        pages.route = "albums"
        tryCompare(pages, "committedRoute", "albums")
        compare(pages.item.objectName, "albumsPage")

        pages.sourceComponent = delayed
        pages.route = "delayed"
        wait(30)
        compare(pages.item.objectName, "albumsPage", "a built page still waits for its first data snapshot")
        tryCompare(pages, "committedRoute", "delayed")
        compare(pages.item.objectName, "delayedPage")

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
