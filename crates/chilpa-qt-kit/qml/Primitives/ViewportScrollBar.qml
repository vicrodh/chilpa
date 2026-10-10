import QtQuick
import QtQuick.Controls.Basic
import "../Theme"

// I keep the thumb beside the viewport's clipped delegates.
ScrollBar {
    id: bar
    property var viewport: null
    property var area: null
    // Views with a full-surface pointer handler can supply its hover state.
    property bool viewportHovered: false
    property real gutter: -Metrics.gap8 // inside the viewport's content margin
    implicitWidth: Metrics.scrollbarWidth
    minimumSize: Math.min(1, Metrics.scrollbarThumbMin / Math.max(height, 1))
    visible: policy !== ScrollBar.AlwaysOff && size < 1 && (hovered || pressed || viewportHovered || !!(area && area.hovered))
    background: Rectangle {
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.surfaceSidebar
        opacity: 0.12
    }
    contentItem: Rectangle {
        implicitWidth: Metrics.scrollbarWidth
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.textTertiary
        opacity: bar.pressed ? 1 : 0.85
    }
    Component.onCompleted: {
        viewport = parent
        area = hoverComponent.createObject(viewport)
        Qt.callLater(function() {
            // Keep the bar inside clipped pages. A positive gutter places it
            // beside an inset list when its parent has a free right margin.
            parent = gutter < 0 ? viewport : viewport.parent
            active = Qt.binding(function() { return hovered || pressed || viewportHovered || !!(area && area.hovered) || !!(viewport && viewport.moving) })
            x = Qt.binding(function() { return (parent === viewport ? 0 : viewport.x) + viewport.width + gutter - width / 2 }) // derived: center the control in the margin
            y = Qt.binding(function() { return parent === viewport ? 0 : viewport.y })
            height = Qt.binding(function() { return viewport.height })
        })
    }
    active: hovered || pressed || viewportHovered || !!(area && area.hovered)
    Component.onDestruction: if (area) area.destroy()
    Component { id: hoverComponent; HoverHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } }
}
