import QtQuick
import QtQuick.Effects
import "../Theme"

// The two shadows a raised surface casts, the wide one and the key one
// close under it, over the place of `target` (a sibling) with its corners.
// Declared before the surface, so they fall under it.
Item {
    id: shadows

    required property Item target
    property real radius: 0

    x: target.x
    y: target.y
    width: target.width
    height: target.height

    RectangularShadow {
        anchors.fill: parent
        radius: shadows.radius
        offset.y: Metrics.shadowElevatedY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }
    RectangularShadow {
        anchors.fill: parent
        radius: shadows.radius
        offset.y: Metrics.shadowKeyY
        blur: Metrics.shadowKeyBlur
        color: Theme.color.shadowElevatedKey
    }
}
