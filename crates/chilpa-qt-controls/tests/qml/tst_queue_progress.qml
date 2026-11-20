import QtQuick
import QtTest
import app.chilpa.kit
import "../../qml/Components"

TestCase {
    name: "QueueProgress"
    when: windowShown
    visible: true
    width: 600; height: 200
    QueueProgress { id: progress; x: 100; y: 50 }
    MouseArea { anchors.fill: parent; z: -1; onClicked: ++test.clicked }
    id: test
    property int clicked: 0
    function init() { QueueActivity.pending = false; QueueActivity.refresh() }
    function cleanup() { QueueActivity.pending = false; QueueActivity.refresh() }
    function test_notice_tracks_work_without_blocking_the_surface() {
        QueueActivity.pending = true
        QueueActivity.refresh()
        tryCompare(progress, "visible", true)
        mouseClick(test, progress.x + progress.width / 2, progress.y + progress.height / 2)
        compare(clicked, 1, "the notice does not consume a click")
        QueueActivity.pending = false
        tryCompare(progress, "visible", false)
    }
    function test_idle_notice_sleeps_and_external_work_wakes_it_again() {
        const before = QueueActivity.refreshes
        wait(350)
        compare(QueueActivity.refreshes, before, "a hidden notice does not poll")
        for (let request = 0; request < 2; ++request) {
            QueueActivity.pending = true
            QueueActivity.refresh()
            tryCompare(progress, "visible", true, 250)
            const working = QueueActivity.refreshes
            tryVerify(() => QueueActivity.refreshes > working)
            QueueActivity.pending = false
            tryCompare(progress, "visible", false)
            const ended = QueueActivity.refreshes
            wait(250)
            compare(QueueActivity.refreshes, ended, "finished work releases the timer")
        }
    }
}
