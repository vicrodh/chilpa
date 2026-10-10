import QtQuick
import QtQuick.Controls.Basic
import "../Primitives"

// Keep a route's viewport position while the screen Loader replaces pages.
Item {
    id: memory
    property var viewport: null
    property string key: ""
    property real pending: -1
    property bool applying: false
    visible: false

    function resume() {
        if (!viewport || pending < 0) return
        const end = Math.max(0, viewport.contentHeight - viewport.height)
        applying = true
        viewport.contentY = Math.min(pending, end)
        applying = false
        if (end >= pending) pending = -1
    }
    onKeyChanged: {
        pending = key ? Visit.recall(key, 0) : -1
        if (pending <= 0) { pending = -1; if (viewport) viewport.contentY = 0 }
        else Qt.callLater(resume)
    }
    onViewportChanged: Qt.callLater(resume)
    Connections {
        target: memory.viewport
        function onContentHeightChanged() { Qt.callLater(memory.resume) }
        function onContentYChanged() {
            if (memory.applying || !memory.key || !memory.viewport) return
            if (memory.pending >= 0) Qt.callLater(memory.resume)
            else Visit.keep(memory.key, memory.viewport.contentY)
        }
        function onMovementStarted() {
            memory.pending = -1
            if (memory.key) Visit.keep(memory.key, memory.viewport.contentY)
        }
    }
    Connections {
        target: memory.viewport ? memory.viewport.ScrollBar.vertical : null
        function onPressedChanged() {
            if (target.pressed) memory.pending = -1
        }
    }
}
