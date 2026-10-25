import QtQuick
import QtTest
import "../../../chilpa-qt-kit/qml/Primitives"

TestCase {
    name: "VolumeTooltip"
    when: windowShown
    visible: true
    width: 400
    height: 200
    LevelSlider {
        id: slider
        x: 40; y: 40; width: 200
        value: 40
        tip: "Volume " + value + "%"
        onMoved: (level) => value = level
    }
    SignalSpy { id: released; target: slider; signalName: "released" }
    function cleanup() {
        mouseMove(slider, -20, -20)
        TipState.hide(slider)
        released.clear()
    }
    function test_visible_tooltip_updates_without_leaving_hover() {
        mouseMove(slider, 50, 10)
        tryCompare(TipState, "shown", true)
        slider.value = 73
        compare(TipState.text, "Volume 73%")
        compare(TipState.shown, true)
        compare(TipState.delay.running, false)
        slider.value = 0
        compare(TipState.text, "Volume 0%")
        compare(TipState.shown, true)
    }
    function test_text_changes_do_not_restart_pending_delay() {
        mouseMove(slider, 50, 10)
        verify(TipState.delay.running)
        wait(TipState.delay.interval / 2)
        slider.value = 61
        tryCompare(TipState, "shown", true, TipState.delay.interval * 0.75)
        compare(TipState.text, "Volume 61%")
    }
    function test_wheel_commits_the_live_value() {
        mouseWheel(slider, 50, 10, 0, 120)
        compare(released.count, 1)
        compare(released.signalArguments[0][0], slider.value)
    }
}
