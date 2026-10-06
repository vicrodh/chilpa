import QtQuick
import QtQuick.Controls.Basic
import QtTest
import "../../../chilpa-qt-kit/qml/Primitives"

TestCase {
    name: "ViewportScrollbar"
    when: windowShown
    visible: true
    width: 500
    height: 400
    Flickable {
        id: view
        x: 20; y: 20; width: 400; height: 300
        contentWidth: width
        contentHeight: 1000
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: bar; gutter: 14 }
    }
    function test_hover_reveals_thumb_in_margin_without_scrolling() {
        tryCompare(bar, "parent", view.parent)
        wait(20)
        compare(bar.viewport, view)
        compare(bar.x + bar.width / 2, view.x + view.width + 14)
        mouseMove(view, 100, 100)
        tryCompare(bar, "active", true)
        compare(view.contentY, 0)
        mouseMove(this, 490, 390)
        tryCompare(bar, "active", false)
    }
}
