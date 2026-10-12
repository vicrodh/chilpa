import QtQuick
import QtQuick.Effects
import "../Theme"

// I draw a raised surface's two shadows or the panel's subtle one under
// `target` (a sibling), following its corners.
Item {
    id: shadows

    required property Item target
    property real radius: 0
    property bool subtle: false

    x: target.x
    y: target.y
    width: target.width
    height: target.height

    RectangularShadow {
        visible: !shadows.subtle
        anchors.fill: parent
        radius: shadows.radius
        offset.y: Metrics.shadowElevatedY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }
    RectangularShadow {
        anchors.fill: parent
        radius: shadows.radius
        offset.y: shadows.subtle ? Metrics.shadowSubtleY : Metrics.shadowKeyY
        blur: shadows.subtle ? Metrics.shadowSubtleBlur : Metrics.shadowKeyBlur
        color: shadows.subtle ? Theme.color.shadowSubtle : Theme.color.shadowElevatedKey
    }
}
