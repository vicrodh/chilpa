import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.link
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/link/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

ApplicationWindow {
    id: window
    width: 900; height: 650; visible: true
    property string musicUrl: "INPUT_URL"
    property int sourceCount: 1
    property string capturePath: "CAPTURE_PATH"
    property bool captured: false
    LinkModal { id: modal }
    // Include the popup surface when capturing its content item in software.
    Rectangle { parent: modal.contentItem; anchors.fill: parent; z: -1; color: Theme.color.surfaceRaised }
    SignalSpy { id: opened; target: Link; signalName: "opened" }
    Timer { interval: 150; running: true; repeat: true; onTriggered: Sources.refresh() }
    TestCase {
        name: "ForeignLink"
        when: window.visible
        function test_link_finds_destinations_without_the_origin_plugin() {
            tryVerify(() => JSON.parse(Sources.homeLibraries(0)).length === window.sourceCount, 6000)
            Link.show()
            tryCompare(modal, "visible", true)
            const field = findChild(modal.contentItem, "linkField")
            field.text = window.musicUrl
            const go = findChild(modal.contentItem, "linkGo")
            verify(go.enabled)
            waitForRendering(go)
            mouseClick(go, go.width / 2, go.height / 2)
            tryVerify(() => Link.resolving || Link.error !== "" || opened.count > 0 || JSON.parse(Link.choices || "[]").length > 0, 2000)
            tryCompare(Link, "resolving", false, 25000)
            console.log("SMOKE foreign link resolved sources=" + window.sourceCount + " error=" + Link.error)
            compare(Link.error, "")
            const choices = JSON.parse(Link.choices || "[]")
            if (window.sourceCount === 1) {
                tryCompare(opened, "count", 1)
                compare(choices.length, 0)
                compare(Link.open, false)
            } else {
                compare(opened.count, 0)
                compare(choices.length, window.sourceCount)
                verify(choices.every(choice => choice.title && choice.source))
                waitForRendering(modal.contentItem)
                verify(modal.contentItem.grabToImage(result => {
                    verify(result.saveToFile(window.capturePath))
                    window.captured = true
                }))
                tryCompare(window, "captured", true)
                Link.choose(1)
                tryCompare(opened, "count", 1)
                compare(Link.open, false)
            }
        }
        function cleanupTestCase() {
            console.log(qtest_results.failCount ? "SMOKE FAIL foreign music link" : "SMOKE foreign music link completed sources=" + window.sourceCount)
            Qt.exit(qtest_results.failCount ? 1 : 0)
        }
    }
    Timer { interval: 35000; running: true; onTriggered: Qt.exit(1) }
}
