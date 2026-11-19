import QtQuick
import QtQuick.Controls.Basic
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Keep the section under the jump bar in place when an earlier async
// block settles. User scrolling chooses a new anchor; no polling is needed.
Item {
    id: position
    required property Flickable viewport
    required property Column column
    required property Repeater blocks
    property string context
    property var anchor: null
    property string identity: ""
    property real anchorY: 0
    property bool applying: false
    property bool following: false
    visible: false
    onContextChanged: reset()
    onEnabledChanged: reset()

    function reset() { following = false; anchor = null }
    // ScrollMemory restores an absolute offset during initial layout.
    // Follow a section only once the listener starts navigating this page.
    function follow() { following = true; capture() }

    // How far the column moved the anchor since it was captured. It is not
    // zero only inside the column's layout pass, before settle().
    function drift() { return anchor && anchor.visible ? column.y + anchor.y - anchorY : 0 }
    function capture(settled) {
        if (applying || !enabled || !following) return
        // The viewport rounds or clamps its offset to the column's new height
        // inside the layout pass. That change is not the listener's: the
        // anchor stays until settle() compensates the blocks that moved.
        if (!settled && Math.abs(drift()) > 0.001) return
        anchor = null
        if (viewport.contentY <= 0) return
        const edge = viewport.contentY + Metrics.jumpOffset
        for (let i = 0; i < blocks.count; i++) {
            const block = blocks.itemAt(i)
            if (block && block.visible && column.y + block.y <= edge) anchor = block
        }
        if (anchor) {
            identity = anchor.section.key || anchor.section.id
            anchorY = column.y + anchor.y
        }
    }
    function jump(id, block) {
        if (id === "about") { viewport.contentY = 0; return }
        if (!block) return
        viewport.contentY = Math.min(column.y + block.y - Metrics.jumpOffset,
            Math.max(0, viewport.contentHeight - viewport.height))
        follow()
    }
    function settle() {
        if (!enabled) return
        const delta = anchor && identity === (anchor.section.key || anchor.section.id) ? drift() : 0
        if (Math.abs(delta) > 0.001) {
            applying = true
            viewport.contentY = Math.max(0, Math.min(viewport.contentY + delta,
                viewport.contentHeight - viewport.height))
            applying = false
        }
        capture(true)
    }
    Connections {
        target: position.enabled ? position.viewport : null
        function onContentYChanged() { position.capture() }
        function onMovementStarted() { position.follow() }
    }
    Connections {
        target: position.enabled ? position.column : null
        function onPositioningComplete() { position.settle() }
    }
    Connections {
        target: position.enabled ? position.viewport.ScrollBar.vertical : null
        function onPressedChanged() { if (target.pressed) position.follow() }
    }
    WheelHandler {
        parent: position.viewport
        enabled: position.enabled
        blocking: false
        target: null
        onWheel: event => { position.follow(); event.accepted = false }
    }
}
