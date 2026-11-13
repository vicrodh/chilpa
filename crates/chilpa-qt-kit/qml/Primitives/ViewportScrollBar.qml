import QtQuick
import QtQuick.Controls.Basic
import "../Theme"

// I keep the thumb in a margin beside the viewport's clipped delegates.
ScrollBar {
    id: bar
    // My explicit thumb dimensions already include the full painted area.
    padding: 0
    property var viewport: null
    property var area: null
    property var viewportArea: null
    property Item overlayParent: null
    // One passive inset layer per control, released with the control.
    property Item insetLayer: null
    readonly property bool inset: parent === viewport || parent === insetLayer
    // A horizontally scrolled table keeps its vertical thumb at the visible edge.
    property real rightEdge: NaN
    // A floating player can cover the viewport without covering its controls.
    property real bottomClearance: 0
    readonly property real reservedBottom: Math.max(0, Math.min(bottomClearance, viewport ? Math.max(0, viewport.height - (sideways ? Metrics.gap4 : 0)) : 0))
    readonly property real horizontalGutter: reservedBottom > 0 ? Math.min(gutter, -height / 2) : gutter // derived: the whole thumb precedes the reserved edge
    readonly property bool sideways: orientation === Qt.Horizontal
    // Views with a full-surface pointer handler can supply its hover state.
    property bool viewportHovered: false
    property real gutter: sideways ? -Metrics.gap4 : -Metrics.gap8 // inside the viewport's content margin
    implicitWidth: sideways ? Metrics.scrollbarThumbMin : Metrics.scrollbarWidth
    implicitHeight: sideways ? Metrics.gap4 : Metrics.scrollbarWidth
    minimumSize: Math.min(1, Metrics.scrollbarThumbMin / Math.max(sideways ? width : height, 1))
    visible: policy !== ScrollBar.AlwaysOff && size < 1 && (policy === ScrollBar.AlwaysOn || hovered || pressed || viewportHovered || !!(area && area.hovered) || !!(viewportArea && viewportArea.hovered))
    background: Rectangle {
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.surfaceSidebar
        opacity: 0.12
    }
    contentItem: Rectangle {
        implicitWidth: sideways ? Metrics.scrollbarThumbMin : Metrics.scrollbarWidth
        implicitHeight: sideways ? Metrics.gap4 : Metrics.scrollbarWidth
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.textTertiary
        opacity: bar.pressed ? 1 : 0.85
    }
    Component.onCompleted: {
        viewport = parent
        viewportArea = hoverComponent.createObject(viewport)
        if (gutter >= 0) {
            area = bridgeComponent.createObject(overlayParent || viewport.parent)
            area.x = Qt.binding(function() { return viewport.x + (sideways ? 0 : viewport.width) })
            area.y = Qt.binding(function() { return viewport.y + (sideways ? Math.max(0, viewport.height - bar.reservedBottom + (bar.reservedBottom > 0 ? bar.horizontalGutter - bar.height / 2 : 0)) : 0) })
            area.width = Qt.binding(function() { return sideways ? viewport.width : gutter + bar.width })
            area.height = Qt.binding(function() { return sideways ? (bar.reservedBottom > 0 ? bar.height : gutter + bar.height) : Math.max(0, viewport.height - bar.reservedBottom) })
        }
        Qt.callLater(function() {
            if (!bar || !bar.viewport) return
            // Keep the bar inside clipped pages. A positive gutter uses a
            // free margin beside the viewport.
            const surface = overlayParent || (gutter < 0 ? viewport : viewport.parent)
            if (surface === viewport) {
                // The attached control otherwise restores the full viewport size.
                insetLayer = layerComponent.createObject(viewport)
                insetLayer.z = Qt.binding(function() { return bar.z })
                parent = insetLayer
            } else parent = surface
            active = Qt.binding(function() { return hovered || pressed || viewportHovered || !!(area && area.hovered) || !!(viewportArea && viewportArea.hovered) || !!(viewport && viewport.moving) })
            if (sideways) {
                x = Qt.binding(function() { return inset ? 0 : viewport.x })
                y = Qt.binding(function() { return (inset ? 0 : viewport.y) + Math.max(0, viewport.height - bar.reservedBottom + bar.horizontalGutter - height / 2) }) // derived: center the control in the margin
                width = Qt.binding(function() { return viewport.width })
                height = Qt.binding(function() { return Metrics.gap4 })
            } else {
                x = Qt.binding(function() { return (inset ? 0 : viewport.x) + (isNaN(rightEdge) ? viewport.width : rightEdge) + gutter - width / 2 }) // derived: center the control in the margin
                y = Qt.binding(function() { return inset ? 0 : viewport.y })
                height = Qt.binding(function() { return Math.max(0, viewport.height - bar.reservedBottom) })
            }
        })
    }
    active: hovered || pressed || viewportHovered || !!(area && area.hovered) || !!(viewportArea && viewportArea.hovered)
    Component.onDestruction: { if (area) area.destroy(); if (viewportArea) viewportArea.destroy(); if (insetLayer) insetLayer.destroy() }
    // A positive gutter lives outside a clipped viewport, so its hover area
    // must be a sibling that reaches the thumb.
    Component { id: layerComponent; Item { anchors.fill: parent } }
    Component { id: hoverComponent; HoverHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } }
    Component { id: bridgeComponent; Item { property alias hovered: bridgeHover.hovered; HoverHandler { id: bridgeHover; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } } }
}
