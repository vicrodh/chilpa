import QtQuick
import QtTest
import "../../../chilpa-qt-kit/qml/Primitives"

TestCase {
    name: "AnimatedCover"
    when: windowShown
    visible: true
    width: 220; height: 220
    AnimatedCover {
        id: cover
        width: 180; height: 180
        colors: ["#221725", "#d17655", "#f8dda7"]
        label: "Daily mix"
    }
    function test_color_moves_only_while_visible() {
        const initial = cover.phase
        tryVerify(() => cover.phase > initial, 500)
        cover.visible = false
        const paused = cover.phase
        wait(120)
        compare(cover.phase, paused)
        cover.visible = true
    }
}
