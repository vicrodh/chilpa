import QtQuick
import QtQuick.Controls.Basic
import QtTest
import "../../qml/Primitives"
import "../../qml/Theme"

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
        ScrollBar.vertical: ViewportScrollBar { id: bar }
    }
    Flickable {
        id: longView
        x: 430; y: 20; width: 50; height: 300
        contentWidth: width; contentHeight: 19000 * 36
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: longBar }
    }
    function test_long_library_has_a_visible_grabbable_thumb() {
        tryCompare(longBar, "parent", longView.parent)
        compare(longBar.width, Metrics.scrollbarWidth)
        compare(longBar.x, longView.x + longView.width)
        verify(!longBar.visible)
        verify(longBar.minimumSize * longBar.height >= Metrics.scrollbarThumbMin)
        mouseMove(longView, 20, 30)
        tryCompare(longBar, "visible", true)
        verify(longBar.contentItem.opacity > 0)
    }
    function test_hover_reveals_thumb_in_margin_without_scrolling() {
        tryCompare(bar, "parent", view.parent)
        wait(20)
        compare(bar.viewport, view)
        compare(bar.x, view.x + view.width)
        verify(!bar.visible)
        mouseMove(view, 100, 100)
        tryCompare(bar, "active", true)
        tryCompare(bar, "visible", true)
        compare(view.contentY, 0)
        mouseMove(this, 490, 390)
        tryCompare(bar, "active", false)
        tryCompare(bar, "visible", false)
    }
}
