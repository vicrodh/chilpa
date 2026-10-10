import QtQuick
import QtQuick.Controls.Basic
import QtTest
import "../../qml/Primitives"
import "../../qml/Theme"

TestCase {
    id: test
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
        MouseArea { id: rowPointer; anchors.fill: parent; hoverEnabled: true }
    }
    Flickable {
        id: wideView
        x: 20; y: 330; width: 400; height: 32
        contentWidth: 1000; contentHeight: height
        clip: true
        ScrollBar.horizontal: ViewportScrollBar { id: wideBar }
    }
    Flickable {
        id: overlayView
        x: 20; y: 365; width: 400; height: 30
        contentWidth: width; contentHeight: 1000
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: overlayBar; overlayParent: test; z: 2 }
    }
    Rectangle { x: 300; y: 365; width: 120; height: 30; z: 1 }
    function test_overlay_panel_keeps_scrollbar_at_page_edge() {
        tryCompare(overlayBar, "parent", this)
        compare(overlayBar.x, overlayView.x + overlayView.width - Metrics.gap8 - overlayBar.width / 2)
        compare(overlayBar.y, overlayView.y)
        verify(overlayBar.z > 1)
    }
    function test_horizontal_thumb_uses_the_reserved_bottom_margin() {
        tryCompare(wideBar, "parent", wideView)
        compare(wideBar.width, wideView.width)
        compare(wideBar.y, wideView.height - Metrics.gap4 - wideBar.height / 2)
        verify(wideBar.y >= 0 && wideBar.y + wideBar.height <= wideView.height)
        mouseMove(wideView, 20, 20)
        tryCompare(wideBar, "visible", true)
    }
    function test_long_library_has_a_visible_grabbable_thumb() {
        tryCompare(longBar, "parent", longView)
        compare(longBar.width, Metrics.scrollbarWidth)
        compare(longBar.x, longView.width - Metrics.gap8 - longBar.width / 2)
        verify(longBar.x >= 0 && longBar.x + longBar.width <= longView.width)
        verify(!longBar.visible)
        verify(longBar.minimumSize * longBar.height >= Metrics.scrollbarThumbMin)
        mouseMove(longView, 20, 30)
        tryCompare(longBar, "visible", true)
        verify(longBar.contentItem.opacity > 0)
    }
    function test_full_surface_pointer_can_reveal_thumb() {
        longBar.viewportHovered = Qt.binding(function() { return rowPointer.containsMouse })
        mouseMove(longView, 12, 120)
        tryCompare(rowPointer, "containsMouse", true)
        tryCompare(longBar, "visible", true)
    }
    function test_hover_reveals_thumb_in_margin_without_scrolling() {
        tryCompare(bar, "parent", view)
        wait(20)
        compare(bar.viewport, view)
        compare(bar.x, view.width - Metrics.gap8 - bar.width / 2)
        verify(bar.x >= 0 && bar.x + bar.width <= view.width)
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
