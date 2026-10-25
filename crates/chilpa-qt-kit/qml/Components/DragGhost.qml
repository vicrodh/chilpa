import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// What follows the pointer while rows are dragged: the track's title over
// its artist and record, or how many tracks when a selection is dragged,
// with a second sheet behind to say it is a pile. Drawn in the layer popups
// are drawn in, so no list clips it; it takes no input.
Item {
    id: ghost

    // The item `at` is measured in.
    property Item source
    property point at
    property int count: 1
    property string title
    property string detail

    readonly property point spot: source ? source.mapToItem(parent, at.x, at.y) : Qt.point(0, 0)

    // The overlay of the source's window: asked of the ghost itself, the
    // answer is lost the moment it has no parent to find a window through.
    parent: source ? source.Overlay.overlay : null
    z: Metrics.tooltipLayer
    x: spot.x + Metrics.gap10
    y: spot.y + Metrics.gap14
    width: lines.width + Metrics.gap22
    height: lines.height + Metrics.gap12

    Rectangle {
        visible: ghost.count > 1
        x: Metrics.gap4
        y: Metrics.gap4
        width: parent.width
        height: parent.height
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised

        Edge {}
    }
    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised

        Edge { border.color: Theme.color.accent }
    }
    Column {
        id: lines
        anchors.centerIn: parent
        spacing: Metrics.gap2

        Text {
            width: Math.min(implicitWidth, Metrics.menuMaxWidth)
            text: ghost.count > 1 ? I18n.trCount("{} track", "{} tracks", ghost.count, I18n.rev) : ghost.title
            elide: Text.ElideRight
            font.pointSize: Type.meta
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: Math.min(implicitWidth, Metrics.menuMaxWidth)
            visible: ghost.count === 1 && ghost.detail !== ""
            text: ghost.detail
            elide: Text.ElideRight
            font.pointSize: Type.micro
            color: Theme.color.textSecondary
        }
    }
}
