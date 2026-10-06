import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

Item {
    id: row
    property var match: ({})
    property string destination: ""
    property string sourceName: ""
    property string qualityLabel: ""
    implicitHeight: details.height + Metrics.gap12
    Rectangle {
        id: badge; width: Math.min(row.width / 3, Math.max(Metrics.button40, name.implicitWidth + Metrics.gap16)) // derived: proportional space within the parent
        height: Metrics.button28; radius: Metrics.radiusRow; color: Theme.color.stateSelected
        Text { id: name; anchors.fill: parent; anchors.margins: Metrics.gap4; text: row.sourceName; color: Theme.color.textPrimary; font.pointSize: Type.meta; font.weight: Type.semibold; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight }
    }
    Column {
        id: details; anchors.left: badge.right; anchors.leftMargin: Metrics.gap12; anchors.right: parent.right; spacing: Metrics.gap4
        Text { width: parent.width; text: (row.match.title || "") + " — " + (row.match.artist || ""); elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body }
        Text { width: parent.width; wrapMode: Text.Wrap; text: row.qualityLabel + " · " + Math.round((row.match.confidence || 0) * 100) + "% · " + row.destination; color: Theme.color.textSecondary; font.pointSize: Type.meta }
    }
}
