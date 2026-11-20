import QtQuick
import QtQuick.Controls.Basic
import "../Theme"
import "../Theme/Viewport.js" as Viewport

// I keep the thumb in a margin beside the viewport's clipped delegates.
ScrollBar {
    id: bar
    padding: 0
    property var viewport: null
    property var area: null
    property var viewportArea: null
    property Item overlayParent: null
    // One passive inset layer per control, released with the control.
    property Item insetLayer: null
    readonly property bool inset: parent === viewport || parent === insetLayer
    // Async component completion may attach me after my first placement. Reclaim
    // the chosen surface when Qt reparents me; no polling or extra inset layers.
    onParentChanged: if (viewport && parent === viewport && (overlayParent || gutter >= 0 || insetLayer)) placement.restart()
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
    function mappedOrigin(surface) { return overlayParent ? Viewport.pointIn(viewport, surface) : Qt.point(viewport.x, viewport.y) }
    Component.onCompleted: {
        viewport = parent
        viewportArea = hoverComponent.createObject(viewport)
        if (gutter >= 0) {
            area = bridgeComponent.createObject(overlayParent || viewport.parent)
            area.x = Qt.binding(function() { return bar.mappedOrigin(area.parent).x + (sideways ? 0 : isNaN(rightEdge) ? viewport.width : rightEdge) })
            area.y = Qt.binding(function() { return bar.mappedOrigin(area.parent).y + (sideways ? Math.max(0, viewport.height - bar.reservedBottom + (bar.reservedBottom > 0 ? bar.horizontalGutter - bar.height / 2 : 0)) : 0) })
            area.width = Qt.binding(function() { return sideways ? viewport.width : gutter + bar.width })
            area.height = Qt.binding(function() { return sideways ? (bar.reservedBottom > 0 ? bar.height : gutter + bar.height) : Math.max(0, viewport.height - bar.reservedBottom) })
        }
        if (!placement.running) placement.start()
    }
    // Rearm at owner completion if this fires before viewport capture.
    Timer {
        id: placement
        interval: 0
        running: true
        onTriggered: {
            if (!bar || !bar.viewport) return
            // Negative gutters stay clipped; positive gutters use the free margin.
            const surface = overlayParent || (gutter < 0 ? viewport : viewport.parent)
            if (surface === viewport && viewport.contentY !== undefined) {
                // The attached control otherwise restores the full viewport size.
                if (!insetLayer) {
                    insetLayer = layerComponent.createObject(viewport)
                    if (!insetLayer) return
                    insetLayer.z = Qt.binding(function() { return bar.z })
                }
                bar.parent = insetLayer
            } else bar.parent = surface
            active = Qt.binding(function() { return hovered || pressed || viewportHovered || !!(area && area.hovered) || !!(viewportArea && viewportArea.hovered) || !!(viewport && viewport.moving) })
            // Both axes share the origin; only the thumb's cross-axis offset differs.
            x = Qt.binding(function() { return (inset ? 0 : bar.mappedOrigin(bar.parent).x) + (sideways ? 0 : (isNaN(rightEdge) ? viewport.width : rightEdge) + gutter - width / 2) })
            y = Qt.binding(function() { return (inset ? 0 : bar.mappedOrigin(bar.parent).y) + (sideways ? Math.max(0, viewport.height - bar.reservedBottom + bar.horizontalGutter - height / 2) : 0) })
            height = Qt.binding(function() { return sideways ? Metrics.gap4 : Math.max(0, viewport.height - bar.reservedBottom) })
            if (sideways) width = Qt.binding(function() { return viewport.width })
        }
    }
    Component.onDestruction: { if (area) area.destroy(); if (viewportArea) viewportArea.destroy(); if (insetLayer) insetLayer.destroy() }
    // A positive gutter needs a sibling hover area outside the clipped viewport.
    Component { id: layerComponent; Item { anchors.fill: parent } }
    Component { id: hoverComponent; HoverHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } }
    Component { id: bridgeComponent; Item { property alias hovered: bridgeHover.hovered; HoverHandler { id: bridgeHover; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } } }
}
