import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.kit

TestCase {
    id: tests
    name: "NativeFrameFence"
    when: windowShown
    visible: true
    width: 120; height: 100
    FrameFence { id: fence }
    SignalSpy { id: presented; target: fence; signalName: "presented" }
    Window {
        id: detached
        width: 80; height: 80; visible: false
        FrameFence { id: hiddenFence }
    }
    SignalSpy { id: hiddenPresented; target: hiddenFence; signalName: "presented" }
    function test_cancel_and_content_tokens() {
        fence.arm(2)
        fence.arm(4)
        tryCompare(presented, "count", 1)
        compare(presented.signalArguments[0][0], 4)
        fence.arm(5)
        tryCompare(presented, "count", 2)
        compare(presented.signalArguments[1][0], 5)
    }
    function test_hidden_window_waits_for_exposure() {
        hiddenFence.arm(10)
        wait(40)
        compare(hiddenPresented.count, 0)
        detached.visible = true
        tryCompare(hiddenPresented, "count", 1)
        compare(hiddenPresented.signalArguments[0][0], 10)
        detached.visible = false
    }
}
