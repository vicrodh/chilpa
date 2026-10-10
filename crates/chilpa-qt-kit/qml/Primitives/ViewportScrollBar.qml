import QtQuick
import QtQuick.Controls.Basic
import "../Theme"

// I keep the thumb in a margin beside the viewport's clipped delegates.
ScrollBar {
    id: bar
    property var viewport: null
    property var area: null
    property Item overlayParent: null
    readonly property bool sideways: orientation === Qt.Horizontal
    // Views with a full-surface pointer handler can supply its hover state.
    property bool viewportHovered: false
    property real gutter: sideways ? -Metrics.gap4 : -Metrics.gap8 // inside the viewport's content margin
    implicitWidth: sideways ? Metrics.scrollbarThumbMin : Metrics.scrollbarWidth
    implicitHeight: sideways ? Metrics.gap4 : Metrics.scrollbarWidth
    minimumSize: Math.min(1, Metrics.scrollbarThumbMin / Math.max(sideways ? width : height, 1))
    visible: policy !== ScrollBar.AlwaysOff && size < 1 && (hovered || pressed || viewportHovered || !!(area && area.hovered))
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
        area = hoverComponent.createObject(viewport)
        Qt.callLater(function() {
            // Keep the bar inside clipped pages. A positive gutter uses a
            // free margin beside the viewport.
            parent = overlayParent || (gutter < 0 ? viewport : viewport.parent)
            active = Qt.binding(function() { return hovered || pressed || viewportHovered || !!(area && area.hovered) || !!(viewport && viewport.moving) })
            if (sideways) {
                x = Qt.binding(function() { return parent === viewport ? 0 : viewport.x })
                y = Qt.binding(function() { return (parent === viewport ? 0 : viewport.y) + viewport.height + gutter - height / 2 }) // derived: center the control in the margin
                width = Qt.binding(function() { return viewport.width })
                height = Qt.binding(function() { return Metrics.gap4 })
            } else {
                x = Qt.binding(function() { return (parent === viewport ? 0 : viewport.x) + viewport.width + gutter - width / 2 }) // derived: center the control in the margin
                y = Qt.binding(function() { return parent === viewport ? 0 : viewport.y })
                height = Qt.binding(function() { return viewport.height })
            }
        })
    }
    active: hovered || pressed || viewportHovered || !!(area && area.hovered)
    Component.onDestruction: if (area) area.destroy()
    Component { id: hoverComponent; HoverHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } }
}
