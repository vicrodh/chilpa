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
    function init() { memory.key = "first"; view.contentHeight = 1800; memory.pending = -1 }

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
