import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P

Rectangle {
    id: row
    required property var entry
    required property int rowIndex
    property string formatId: ""
    property bool canDownload: false
    signal download()

    height: Metrics.settingsRow
    color: rowIndex % 2 ? Theme.color.stateStripe : "transparent"
    Row {
        anchors.fill: parent; spacing: Metrics.gap12
        Text { width: Metrics.gap24; text: String(row.entry.number || ""); color: Theme.color.textTertiary; font.pointSize: Type.body; anchors.verticalCenter: parent.verticalCenter }
        Text {
            width: Math.max(0, parent.width - Metrics.gap24 - duration.width - status.width - button.width - Metrics.gap12 * 4) // derived: four gaps between five cells
            text: row.entry.title + (row.entry.version ? " (" + row.entry.version + ")" : "")
            textFormat: Text.PlainText; elide: Text.ElideRight
            color: Theme.color.textPrimary; font.pointSize: Type.body; anchors.verticalCenter: parent.verticalCenter
        }
        Text { id: duration; text: Math.floor((row.entry.duration || 0) / 60) + ":" + String((row.entry.duration || 0) % 60).padStart(2, "0"); color: Theme.color.textSecondary; font.pointSize: Type.body; anchors.verticalCenter: parent.verticalCenter }
        Text {
            id: status
            text: (row.entry.downloaded_formats || []).indexOf(Number(row.formatId)) >= 0 ? I18n.tr("Downloaded", I18n.rev) : ""
            color: Theme.color.textSecondary; font.pointSize: Type.meta; anchors.verticalCenter: parent.verticalCenter
        }
        P.PillButton { id: button; text: I18n.tr("Download", I18n.rev); enabled: row.canDownload; onClicked: row.download() }
    }
}
