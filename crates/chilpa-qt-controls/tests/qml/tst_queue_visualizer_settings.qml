import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Patterns"

TestCase {
    id: test
    name: "QueueVisualizerSettings"
    when: windowShown
    visible: true
    width: 300
    height: 800
    property var card: null
    Component { id: maker; ListenNow { width: 232; height: 780 } }
    function init() {
        SettingsModel.preferences = ({})
        AudioOutput.pathReads = 0
        AudioOutput.pathAnswer = '{"state":"idle"}'
        card = maker.createObject(test)
        verify(card !== null)
    }
    function cleanup() { card.destroy(); wait(0) }
    function test_path_off_releases_content_and_stops_reads() {
        const host = findChild(card, "queueSignalPathHost")
        verify(card.signalPath && host.item !== null)
        card.toggleSignalPath()
        compare(host.item, null)
        compare(card.pathDocument, "{}")
        const reads = AudioOutput.pathReads
        for (let i = 0; i < 5; ++i) card.refreshPath()
        compare(AudioOutput.pathReads, reads)
        card.toggleSignalPath()
        verify(host.item !== null)
        compare(AudioOutput.pathReads, reads + 1)
        card.visible = false
        compare(host.item, null)
        card.refreshPath()
        compare(AudioOutput.pathReads, reads + 1)
        card.visible = true
        compare(AudioOutput.pathReads, reads + 2)
        card.show("lyrics")
        compare(host.item, null)
        card.refreshPath()
        compare(AudioOutput.pathReads, reads + 2)
    }
    function test_both_toggles_survive_recreation_and_fit_the_row() {
        const route = findChild(card, "queueSignalPathToggle")
        const visual = findChild(card, "queueVisualizationToggle")
        const tabs = findChild(card, "queuePanelTabs")
        verify(route.x > visual.x)
        const left = visual.mapToItem(card, 0, 0).x
        verify(left >= tabs.x + tabs.width, "toggles leave the tab selector clear")
        verify(route.mapToItem(card, route.width, 0).x <= card.width)
        card.toggleVisualization()
        card.toggleSignalPath()
        compare(SettingsModel.kept("ui.queue.visualizer"), "true")
        compare(SettingsModel.kept("ui.queue.signalPath"), "false")
        card.destroy()
        wait(0)
        card = maker.createObject(test)
        verify(card.visualization)
        verify(!card.signalPath)
        compare(findChild(card, "queueSignalPathHost").item, null)
    }
}
