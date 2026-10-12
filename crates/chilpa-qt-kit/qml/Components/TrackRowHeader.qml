import QtQuick
import "../Theme"

// The header's content stays behind the list's one pointer. Its disc
// actions are drawn here and that pointer resolves their clicks.
Item {
    id: header
    required property var row
    property bool actionsEnabled: false
    readonly property Item composerLabel: content.item ? content.item.composerLabel : null
    readonly property Item actionArea: content.item && content.item.discActions ? content.item.actionArea : null

    function actionAt(from, x, y) {
        const buttons = actionArea
        if (!buttons || !buttons.visible) return 0
        const point = buttons.mapFromItem(from, x, y)
        if (point.y < 0 || point.y >= buttons.height) return 0
        for (let index = 0; index < 3; index++) {
            const left = index * (Metrics.button30 + Metrics.gap4)
            if (point.x >= left && point.x < left + Metrics.button30) return index + 1
        }
        return 0
    }

    x: row.cols.indexX
    width: row.width - x
    height: row.height
    Loader {
        id: content
        anchors.fill: parent
        active: header.row.header
        sourceComponent: TrackGroupHeader {
            objectName: "groupHeader"
            of: header.row
            discActions: header.actionsEnabled && !header.row.headerWork && !header.row.record
            actionHovered: header.row.actionHovered
        }
    }
}
