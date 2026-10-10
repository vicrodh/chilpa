import QtQuick
import QtQuick.Controls.Basic
import "../Theme"

// I keep the thumb beside the viewport's clipped delegates.
ScrollBar {
    id: bar
    property var viewport: null
    property var area: null
    property real gutter: Metrics.scrollbarWidth / 2 // its left edge starts after the rows
    implicitWidth: Metrics.scrollbarWidth
    minimumSize: Math.min(1, Metrics.scrollbarThumbMin / Math.max(height, 1))
    visible: policy !== ScrollBar.AlwaysOff && size < 1 && (hovered || pressed || !!(area && area.hovered))
    background: Rectangle {
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.surfaceSidebar
        opacity: 0.12
    }
    contentItem: Rectangle {
        implicitWidth: Metrics.scrollbarWidth
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.strokeHairline
        opacity: bar.pressed ? 1 : 0.8
    }
    Component.onCompleted: {
        viewport = parent
        area = hoverComponent.createObject(viewport)
        Qt.callLater(function() {
            // The default bar lives in the open margin; explicit inset bars
            // stay inside the viewport when a panel has no margin.
            parent = gutter < 0 ? viewport : viewport.parent
            active = Qt.binding(function() { return hovered || pressed || !!(area && area.hovered) || !!(viewport && viewport.moving) })
            x = Qt.binding(function() { return (parent === viewport ? 0 : viewport.x) + viewport.width + gutter - width / 2 }) // derived: center the control in the margin
            y = Qt.binding(function() { return parent === viewport ? 0 : viewport.y })
            height = Qt.binding(function() { return viewport.height })
        })
    }
    active: hovered || pressed || !!(area && area.hovered)
    Component.onDestruction: if (area) area.destroy()
    Component { id: hoverComponent; HoverHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } }
}
