import QtQuick
import QtTest
import "../../qml/Theme"
import "../../qml/Primitives"

// A line cut short says itself whole when the pointer rests on it, also
// under its row's hover area, and goes quiet when the pointer leaves it; a
// line that fits says nothing.
TestCase {
    name: "CutTextTips"
    when: windowShown
    visible: true
    width: 400
    height: 200

    Item {
        id: holder
        anchors.fill: parent
        CutTextTips { watching: holder }
        Item {
            width: 300
            height: 40
            Text { id: cut; width: 80; text: "[1982] The Number of the Beast, 5644.2, Vinyl"; elide: Text.ElideMiddle }
            Text { id: fits; x: 150; width: 120; text: "Iron Maiden"; elide: Text.ElideRight }
            MouseArea { anchors.fill: parent; hoverEnabled: true }
        }
    }

    function test_a_cut_line_says_itself_whole_under_its_rows_hover_area() {
        verify(cut.truncated)
        mouseMove(holder, 20, 10)
        mouseMove(holder, 22, 12)
        tryCompare(TipState, "target", cut)
        compare(TipState.text, cut.text)
        mouseMove(holder, 200, 10)
        tryCompare(TipState, "target", null)
        wait(Metrics.cutTextLook * 3)
        compare(TipState.target, null, "a line that fits says nothing")
    }
}
