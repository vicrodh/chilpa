// hot-path: one per visible track of a list kept in its own order.
import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

// Moving a track up or down in a list kept in its own order, before the
// track's number and always in sight: one arrow over the other in a rich
// row, side by side in a compact one. It has no mouse handling of its own:
// the list tells it which one the pointer is on and resolves the click.
Item {
    id: move

    property bool stacked: false
    // 1 up, 2 down, 0 neither: the one under the pointer.
    property int hovered: 0

    readonly property int cellWidth: stacked ? Metrics.colMoveRich : Metrics.moveCellCompact
    readonly property int cellHeight: stacked ? Metrics.moveCell : height

    function explain() {
        if (hovered === 0)
            TipState.hide(move)
        else
            TipState.show(hovered === 1 ? I18n.tr("Move up", I18n.rev) : I18n.tr("Move down", I18n.rev), move)
    }

    width: stacked ? Metrics.colMoveRich : Metrics.colMoveCompact
    onHoveredChanged: explain()

    Repeater {
        model: 2

        Item {
            id: cell
            required property int index
            readonly property bool pointed: move.hovered === index + 1

            x: move.stacked ? 0 : index * move.cellWidth
            y: move.stacked ? move.height / 2 + (index - 1) * move.cellHeight : 0
            width: move.cellWidth
            height: move.cellHeight

            Rectangle {
                anchors.fill: parent
                radius: Metrics.radiusItem
                visible: cell.pointed
                color: Theme.color.stateHover
            }
            Icon {
                anchors.centerIn: parent
                glyph: cell.index === 0 ? Icons.chevronUp : Icons.chevronDown
                size: move.stacked ? Metrics.iconSm : Metrics.iconXs
                color: cell.pointed ? Theme.color.textPrimary : Theme.color.textTertiary
            }
        }
    }
}
