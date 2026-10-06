import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../../chilpa-qt/qml/Components"

// Where the window shows the A-B loop and the timer: the seek bar and the
// foot of the queue.
TestCase {
    name: "LoopAndTimerPlaces"
    when: windowShown
    visible: true
    width: 400
    height: 200

    SeekBar { id: bar; width: 400; duration: 200; position: 10 }
    QueueFooter { id: foot; y: 60; width: 400 }
    SignalSpy { id: asked; target: foot; signalName: "timer" }

    function marks() {
        const found = []
        const collect = (item) => { for (const child of item.children) { if (child.objectName === "loopMark") found.push(child); collect(child) } }
        collect(bar)
        return found.map(mark => Math.round(mark.x + mark.width / 2)).sort((a, b) => a - b)
    }

    function test_the_seek_bar_marks_the_loop_where_it_is() {
        compare(marks(), [], "no loop, no marks")
        bar.loopStart = 50
        compare(marks(), [100], "its start, a quarter of the way")
        bar.loopEnd = 150
        compare(marks(), [100, 300])
        bar.duration = 0
        compare(marks(), [], "nowhere to put them on a track of unknown length")
        bar.duration = 200
        bar.loopStart = -1
        bar.loopEnd = -1
    }

    function test_the_foot_of_the_queue_shows_what_is_left_of_the_timer() {
        const button = findChild(foot, "queueTimer")
        compare(button.text, "Timer")
        foot.sleepLeft = "29m"
        compare(button.text, "29m")
        mouseClick(button)
        compare(asked.count, 1)
    }
}
