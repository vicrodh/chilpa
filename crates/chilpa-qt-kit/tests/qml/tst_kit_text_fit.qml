import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The kit's own pieces ask for the room their words take, so the screens
// that measure them can give it.
TestCase {
    id: test
    name: "KitTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 600
    height: 400

    Component { id: pillFactory; PillButton { glyph: Icons.plus; text: "Add to playlist" } }

    // A hidden pill, as a screen uses one to measure a column, asks for the
    // width the shown one takes, its glyph included.
    function test_a_hidden_pill_asks_for_its_glyph_too() {
        const shown = createTemporaryObject(pillFactory, test)
        const hidden = createTemporaryObject(pillFactory, test, { visible: false })
        verify(shown.implicitWidth > 0)
        compare(hidden.implicitWidth, shown.implicitWidth, "the glyph and its gap count while hidden")
        const wrapped = createTemporaryObject(pillFactory, test, { visible: false, wrapText: true, width: 120 })
        const wrappedShown = createTemporaryObject(pillFactory, test, { wrapText: true, width: 120 })
        compare(wrapped.implicitHeight, wrappedShown.implicitHeight, "the wrapped label leaves the glyph its room while hidden")
    }
}
