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
    function test_notice_tracks_work_without_blocking_the_surface() {
        QueueActivity.busy = true
        tryCompare(progress, "visible", true)
        mouseClick(test, progress.x + progress.width / 2, progress.y + progress.height / 2)
        compare(clicked, 1, "the notice does not consume a click")
        QueueActivity.busy = false
        tryCompare(progress, "visible", false)
    }
}
