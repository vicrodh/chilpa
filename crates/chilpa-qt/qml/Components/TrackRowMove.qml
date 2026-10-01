// hot-path: one per visible track of a list kept in its own order.
import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// Moving a track up or down in a list kept in its own order, in the place
// of the track's number and always in sight. Each arrow is its whole cell,
// as tall as the row. It has no mouse handling of its own: the list tells
// it which one the pointer is on and resolves the click.
Item {
    id: move

    // 1 up, 2 down, 0 neither: the one under the pointer.
    property int hovered: 0

    function explain() {
        if (hovered === 0)
            TipState.hide(move)
        else
            TipState.show(hovered === 1 ? I18n.tr("Move up", I18n.rev) : I18n.tr("Move down", I18n.rev), move)
    }

    width: Metrics.colMove
    onHoveredChanged: explain()

    Repeater {
        model: 2

        Item {
            id: cell
            required property int index
            readonly property bool pointed: move.hovered === index + 1

            x: index * Metrics.rowAction
            width: Metrics.rowAction
            height: move.height

            Rectangle {
                anchors.centerIn: parent
                width: Metrics.button24
                height: Math.min(width, parent.height)
                radius: Metrics.radiusItem
                visible: cell.pointed
                color: Theme.color.stateHover
            }
            Icon {
                anchors.centerIn: parent
                glyph: cell.index === 0 ? Icons.chevronUp : Icons.chevronDown
                size: Metrics.iconSm
                color: cell.pointed ? Theme.color.textPrimary : Theme.color.textTertiary
            }
        }
    }
}
