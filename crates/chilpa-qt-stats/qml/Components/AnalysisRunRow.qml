import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: row
    required property var entry
    required property string stamp
    required property string status
    signal asked()
    height: Charts.inventoryRunHeight
    radius: Metrics.radiusControl
    color: Theme.color.stateHover
    Column {
        anchors.left: parent.left; anchors.right: showTracks.left; anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Metrics.gap12; anchors.rightMargin: Metrics.gap12; spacing: Metrics.gap4
        Text { width: parent.width; text: row.stamp + " · " + (row.entry.kind === "mood" ? I18n.tr("Mood", I18n.rev) : I18n.tr("Standard analysis", I18n.rev)) + " · " + row.status; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
        Text { width: parent.width; text: I18n.tr("Processed", I18n.rev) + ": " + row.entry.processed + " / " + row.entry.total + " · " + I18n.tr("Saved", I18n.rev) + ": " + row.entry.saved + " · " + I18n.tr("Failed", I18n.rev) + ": " + row.entry.failed + " · " + I18n.tr("Previously saved", I18n.rev) + ": " + row.entry.previous; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        Text { width: parent.width; text: row.entry.error || row.entry.folder || row.entry.last; elide: Text.ElideMiddle; font.pointSize: Type.meta; color: row.entry.error ? Theme.color.statusWarning : Theme.color.textTertiary }
    }
    PillButton { id: showTracks; anchors.right: parent.right; anchors.rightMargin: Metrics.gap12; anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("View tracks", I18n.rev); onClicked: row.asked() }
}
