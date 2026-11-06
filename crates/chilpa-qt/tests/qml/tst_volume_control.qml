import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/controls/qml/Components"

TestCase {
    name: "VolumeControl"
    when: windowShown
    visible: true
    width: 400
    height: 400
    VolumeControl { id: control; x: 150; y: 250 }

    function test_drag_applies_live_and_saves_once_on_release() {
        Player.volumeLocked = false
        Player.volumeSaves = 0
        Player.volume = 40
        mouseClick(control, control.width / 2, control.height / 2)
        const slider = findChild(control, "volumeLevel")
        verify(slider !== null)
        tryCompare(slider, "visible", true)
        mousePress(slider, slider.width / 2, slider.height * 0.7)
        compare(Player.volume, 30)
        mouseMove(slider, slider.width / 2, slider.height * 0.2)
        compare(Player.volume, 80)
        compare(Player.volumeSaves, 0)
        mouseRelease(slider, slider.width / 2, slider.height * 0.2)
        compare(Player.volumeSaves, 1)
        mouseClick(control, -100, -100)
    }

    function test_peek_shows_the_level_and_leaves_after_the_last_change() {
        Player.volumeLocked = false
        Player.volume = 40
        control.peekMs = 150
        const glance = findChild(control, "volumeGlance")
        verify(glance !== null)
        control.peek()
        tryCompare(glance, "visible", true)
        wait(100)
        Player.volume = 45
        control.peek()
        wait(100)
        verify(glance.visible) // the second change restarted the wait
        tryCompare(glance, "visible", false, 1000)
    }

    function test_peek_does_nothing_while_the_volume_is_locked() {
        Player.volumeLocked = true
        control.peekMs = 150
        const glance = findChild(control, "volumeGlance")
        control.peek()
        wait(50)
        verify(!glance.visible)
        Player.volumeLocked = false
    }
}
