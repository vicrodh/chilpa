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

    function capture() {
        if (applying || !enabled || !following) return
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
        if (anchor && anchor.visible && identity === (anchor.section.key || anchor.section.id)) {
            const delta = column.y + anchor.y - anchorY
            if (Math.abs(delta) > 0.001) {
                applying = true
                viewport.contentY = Math.max(0, Math.min(viewport.contentY + delta,
                    viewport.contentHeight - viewport.height))
                applying = false
            }
        }
        capture()
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
