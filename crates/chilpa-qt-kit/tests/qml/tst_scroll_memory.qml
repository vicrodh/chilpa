import QtQuick
import QtQuick.Controls.Basic
import QtTest
import "../../qml/Components"
import "../../qml/Primitives"

TestCase {
    name: "ScrollMemory"
    when: windowShown
    visible: true
    width: 300; height: 300
    Flickable {
        id: view; width: 240; height: 200; contentWidth: width; contentHeight: 1800
        ScrollBar.vertical: ViewportScrollBar { id: bar }
    }
    ScrollMemory { id: memory; viewport: view; key: "first" }
    ListView {
        id: headed
        property int headerHeight: 72
        width: 240; height: 200; topMargin: 24; model: 40
        header: Rectangle { width: 240; height: headed.headerHeight }
        delegate: Rectangle { width: 240; height: 32 }
    }
    function init() { memory.viewport = view; memory.key = "first"; view.contentHeight = 1800; memory.pending = -1 }

    function test_a_new_page_keeps_its_header_and_top_margin() {
        memory.viewport = headed
        memory.key = "header"
        wait(20)
        compare(headed.contentY, headed.originY - headed.topMargin,
            "zero means the page's beginning, not an absolute content coordinate")
        verify(headed.headerItem.mapToItem(headed, 0, 0).y >= 0)
    }
    function test_saved_positions_are_relative_to_the_view_origin() {
        headed.forceLayout()
        memory.viewport = headed
        memory.key = "relative"
        wait(20)
        const start = headed.originY - headed.topMargin
        headed.contentY = start + 300
        compare(Visit.recall("relative", -1), 300)
    }
    function test_loading_header_keeps_the_page_at_the_beginning() {
        headed.headerHeight = 280
        memory.viewport = headed
        memory.key = "loading-header"
        wait(30)
        headed.headerHeight = 72
        wait(30)
        compare(headed.contentY, headed.originY - headed.topMargin)
        verify(headed.headerItem.mapToItem(headed, 0, 0).y >= 0)
    }

    function test_restores_a_route_after_delayed_content() {
        memory.pending = -1
        view.contentY = 600
        compare(Visit.recall("first", 0), 600)
        memory.key = "second"
        view.contentHeight = 200
        memory.key = "first"
        wait(20)
        compare(view.contentY, 0)
        view.contentHeight = 1800
        tryCompare(view, "contentY", 600)
    }
    function test_new_route_keeps_manual_scroll() {
        memory.key = "fresh"
        view.contentY = 120
        compare(Visit.recall("fresh", 0), 120)
    }
    function test_dragging_the_thumb_cancels_pending_restore() {
        Visit.keep("saved", 600)
        memory.key = "saved"
        mousePress(bar.contentItem, bar.contentItem.width / 2, bar.contentItem.height / 2)
        tryCompare(memory, "pending", -1)
        mouseRelease(bar.contentItem, bar.contentItem.width / 2, bar.contentItem.height / 2)
    }
}
