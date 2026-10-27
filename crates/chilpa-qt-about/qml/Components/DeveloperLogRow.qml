import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: row
    property var entry: ({})
    property bool heading: false
    implicitHeight: heading ? Metrics.rowGroup : Metrics.rowRecord
    Row {
        width: parent.width
        spacing: Metrics.gap8
        Text {
            width: Metrics.colPlaysRich
            text: row.heading ? row.entry.time : Qt.formatTime(new Date(Number(row.entry.time || 0) * 1000), "hh:mm:ss")
            font.pointSize: Type.small
            color: Theme.color.textSecondary
            elide: Text.ElideRight
        }
        Text { width: Metrics.colPlaysRich; text: row.entry.level || ""; font.pointSize: Type.small; color: Theme.color.textSecondary; elide: Text.ElideRight }
        Text { width: Metrics.selectWidth / 2; text: row.entry.target || ""; font.pointSize: Type.small; color: Theme.color.textSecondary; elide: Text.ElideRight } // derived: compact target column
        Text {
            width: row.width - Metrics.colPlaysRich * 2 - Metrics.selectWidth / 2 - Metrics.gap8 * 3 // derived: remaining width after three fixed columns and gaps
            text: row.entry.message || ""
            font.pointSize: Type.meta
            color: Theme.color.textPrimary
            wrapMode: Text.WrapAnywhere
            maximumLineCount: 3
            elide: Text.ElideRight
        }
    }
    HoverHandler { onHoveredChanged: hovered && !row.heading ? TipState.show(row.entry.message || "", row) : TipState.hide(row) }
}
