import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I share one group toolbar across the recycled rows of a track list.
Row {
    id: root
    property int row: -1
    property var item
    property TrackListModel model
    spacing: Metrics.gap2

    IconButton {
        size: Metrics.button24
        glyph: Icons.playerPlayFilled
        filled: true
        tip: I18n.tr("Play this group", I18n.rev)
        onClicked: root.model.groupAction(root.row, "play", root.item.headerLabel)
    }
    IconButton {
        id: queueButton
        size: Metrics.button24
        glyph: Icons.playlistAdd
        tip: I18n.tr("Queue this group", I18n.rev)
        onClicked: {
            const at = queueButton.mapToItem(root, width - Metrics.menuWidth, height + Metrics.gap4)
            menu.showGroup(root.row, root.item.headerLabel, at.x, at.y)
        }
    }
    QueueMenu { id: menu; model: root.model }
}
