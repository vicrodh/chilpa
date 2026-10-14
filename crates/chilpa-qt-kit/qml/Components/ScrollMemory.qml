import QtQuick
import QtQuick.Controls.Basic
import "../Primitives"

// I store distance from the viewport origin, including its top margin.
Item {
    id: memory
    property var viewport: null
    property string key: ""
    property real pending: -1
    property bool applying: false
    visible: false

    function origin() { return viewport ? (viewport.originY || 0) - (viewport.topMargin || 0) : 0 }
    function remember() {
        if (key && viewport && !applying && pending < 0)
            Visit.keep(key, Math.max(0, viewport.contentY - origin()))
    }
    function restore() {
        pending = key ? Visit.recall(key, 0) : -1
        resume()
    }
    function resume() {
        if (!viewport || pending < 0) return
        if (typeof viewport.forceLayout === "function") viewport.forceLayout()
        const start = origin()
        const end = Math.max(0, viewport.contentHeight - viewport.height
            + (viewport.topMargin || 0) + (viewport.bottomMargin || 0))
        applying = true
        viewport.contentY = start + Math.min(pending, end)
        applying = false
        if (end >= pending) pending = -1
    }
    onKeyChanged: restore()
    onViewportChanged: if (viewport) Qt.callLater(restore)
    Connections {
        target: memory.viewport
        ignoreUnknownSignals: true
        function onContentHeightChanged() { Qt.callLater(memory.resume) }
        function onOriginYChanged() { Qt.callLater(memory.resume) }
        function onContentYChanged() {
            if (memory.applying || !memory.key || !memory.viewport) return
            if (memory.pending >= 0) Qt.callLater(memory.resume)
            else memory.remember()
        }
        function onMovementStarted() {
            memory.pending = -1
            memory.remember()
        }
    }
    Connections {
        target: memory.viewport ? memory.viewport.ScrollBar.vertical : null
        function onPressedChanged() {
            if (target.pressed) memory.pending = -1
        }
    }
    WheelHandler {
        parent: memory.viewport
        target: null
        onWheel: event => { memory.pending = -1; event.accepted = false }
    }
}
