import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"

// While the plugins start, the splash covers the window with the logo over
// the name, the bar and what is loading.
TestCase {
    name: "StartupSplash"; when: windowShown; visible: true; width: 900; height: 700
    StartupSplash { id: splash; anchors.fill: parent }
    property var before

    function initTestCase() { before = Sources.state }
    function cleanupTestCase() { Sources.state = before }

    function test_the_logo_tops_the_splash_while_the_plugins_start() {
        Sources.state = Object.assign({}, before, { starting: true, startup: { total: 2, done: 1, name: "Radio" } })
        tryCompare(splash, "opacity", 1)
        const logo = findChild(splash, "splashLogo")
        verify(logo.visible)
        tryCompare(logo, "status", Image.Ready)
        compare(logo.y, 0, "the logo is the first thing in the column, over the name and the bar")
    }
}
