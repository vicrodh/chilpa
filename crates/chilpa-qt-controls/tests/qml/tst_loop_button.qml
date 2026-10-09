import QtQuick
import QtTest
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// The A-B button beside the transport: one picture in three layers. The
// first click lights the A, the second the B too, the third lets the loop
// go; the frame never lights.
TestCase {
    name: "LoopButton"
    when: windowShown
    visible: true
    width: 200
    height: 100

    LoopButton { id: loop }

    function part(name) {
        return findChild(loop, name)
    }

    // The double paints every token grey; the accent gets its own here.
    function initTestCase() {
        ThemeModel.palette = { "accent": "#d8323a", "text.secondary": "#a1a9b1" }
        ThemeModel.rev += 1
    }

    function cleanup() {
        Controls.clearLoop()
    }

    function test_each_click_lights_one_more_letter() {
        const a = part("loopA"), b = part("loopB")
        verify(a !== null && b !== null)
        verify(!Qt.colorEqual(a.color, Theme.color.accent), "nothing lit before the first click")
        verify(!Qt.colorEqual(b.color, Theme.color.accent))
        mouseClick(loop)
        compare(Controls.loopState, 1)
        verify(Qt.colorEqual(a.color, Theme.color.accent), "the first click lights the A")
        verify(!Qt.colorEqual(b.color, Theme.color.accent), "and only the A")
        mouseClick(loop)
        compare(Controls.loopState, 2)
        verify(Qt.colorEqual(a.color, Theme.color.accent), "a whole segment lights both")
        verify(Qt.colorEqual(b.color, Theme.color.accent))
        compare(loop.checked, false, "the frame is never lit")
        mouseClick(loop)
        compare(Controls.loopState, 0, "the third click lets the loop go")
        verify(!Qt.colorEqual(a.color, Theme.color.accent))
    }

    function test_the_three_layers_are_the_same_size() {
        compare(part("loopA").size, loop.iconSize)
        compare(part("loopB").size, loop.iconSize)
        compare(part("loopA").iconFamily, SourceGlyphs.font)
        compare(loop.iconFamily, SourceGlyphs.font)
    }
}
