import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I share one group toolbar across the recycled rows of a track list.
Row {
    id: root
    property ListView viewport: null
    property int row: viewport ? viewport.hovered : -1
    property var item: viewport && row >= 0 ? viewport.itemAtIndex(row) : null
    visible: !viewport || enabled && item && item.header && (item.headerWork || item.record)
    y: viewport && item ? viewport.y + item.y - viewport.contentY + (item.height - height) / 2 : 0
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
