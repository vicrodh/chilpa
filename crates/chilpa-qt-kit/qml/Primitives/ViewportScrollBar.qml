import QtQuick
import QtQuick.Controls.Basic
import "../Theme"

// I keep the thumb in the viewport's gutter, outside its clipped delegates.
ScrollBar {
    id: bar
    property var viewport: null
    property var area: null
    property real gutter: -Metrics.gap8 // inside the viewport, where clipping cannot hide it
    implicitWidth: Metrics.scrollbarWidth
    minimumSize: Math.min(1, Metrics.scrollbarThumbMin / Math.max(height, 1))
    visible: policy !== ScrollBar.AlwaysOff && size < 1
    background: Rectangle {
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: Theme.color.surfacePanel
        border.color: Theme.color.strokeFrame
        border.width: Metrics.scrollbarBorder
        opacity: 0.8
    }
    contentItem: Rectangle {
        implicitWidth: Metrics.scrollbarWidth
        radius: Metrics.scrollbarWidth / Metrics.gap2
        color: bar.pressed ? Theme.color.accentPressed : bar.hovered ? Theme.color.accentHover : Theme.color.accent
        opacity: 0.9
    }
    Component.onCompleted: {
        viewport = parent
        area = hoverComponent.createObject(viewport)
        Qt.callLater(function() {
            // Inset bars stay inside their viewport, out of parent positioners.
            parent = gutter < 0 ? viewport : viewport.parent
            active = Qt.binding(function() { return hovered || pressed || !!(area && area.hovered) || !!(viewport && viewport.moving) })
            x = Qt.binding(function() { return (parent === viewport ? 0 : viewport.x) + viewport.width + gutter - width / 2 }) // derived: center the control in the margin
            y = Qt.binding(function() { return parent === viewport ? 0 : viewport.y })
            height = Qt.binding(function() { return viewport.height })
        })
    }
    active: hovered || pressed || !!(area && area.hovered) || !!(viewport && viewport.moving)
    Component.onDestruction: if (area) area.destroy()
    Component { id: hoverComponent; HoverHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad } }
}
