import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Rectangle {
    id: row
    required property var entry
    readonly property var track: entry || ({title:"",artist:"",album:""})
    property bool alternate: false
    height: Metrics.settingsRow
    radius: Metrics.radiusControl
    color: alternate ? Theme.color.stateHover : "transparent"
    function date(value) { return value ? Qt.formatDateTime(new Date(value * 1000), Qt.DefaultLocaleShortDate) : "" }
    Row {
        anchors.fill: parent; anchors.leftMargin: Metrics.gap12; anchors.rightMargin: Metrics.gap12; spacing: Metrics.gap12
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - standard.width - mood.width - Metrics.gap24
            Text { width: parent.width; text: row.track.title; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
            Text { width: parent.width; text: row.track.artist + (row.track.album ? " · " + row.track.album : ""); elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }
        Column {
            id: standard
            anchors.verticalCenter: parent.verticalCenter
            Text { text: row.track.result === "failed" ? I18n.tr("Failed in this run", I18n.rev) : row.track.standard ? I18n.tr("Processed", I18n.rev) : row.track.standard_at ? I18n.tr("Needs update", I18n.rev) : I18n.tr("Pending", I18n.rev); font.pointSize: Type.meta; color: row.track.result === "failed" ? Theme.color.statusWarning : Theme.color.textSecondary }
            Text { text: row.date(row.track.processed_at || row.track.standard_at); font.pointSize: Type.meta; color: Theme.color.textTertiary }
        }
        Column {
            id: mood
            anchors.verticalCenter: parent.verticalCenter
            Text { text: row.track.mood ? I18n.tr("Mood stored", I18n.rev) : I18n.tr("Mood pending", I18n.rev); font.pointSize: Type.meta; color: row.track.mood ? Theme.color.textSecondary : Theme.color.textTertiary }
            Text { text: row.date(row.track.mood_at); font.pointSize: Type.meta; color: Theme.color.textTertiary }
        }
    }
}
