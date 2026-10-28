import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The column header and the well of rows, with QBZ's geometry: 80, 48 and 140
// wide, then the message, 12 of padding and 10 between.
Item {
    id: log
    property var rows: []
    property bool autoTail: false
    readonly property int colTime: 80 // intrinsic: QBZ's time column
    readonly property int colLevel: 48 // intrinsic: QBZ's level column
    readonly property int colTarget: 140 // intrinsic: QBZ's target column

    Row {
        id: header
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap12
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        spacing: Metrics.gap10
        Text { width: log.colTime; text: I18n.tr("Time", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small; font.weight: Type.semibold }
        Text { width: log.colLevel; text: I18n.tr("Level", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small; font.weight: Type.semibold }
        Text { width: log.colTarget; text: I18n.tr("Target", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small; font.weight: Type.semibold }
        Text { text: I18n.tr("Message", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small; font.weight: Type.semibold }
    }
    // A step darker than the panel it sits on: the list reads as a well.
    Rectangle {
        anchors.top: header.bottom
        anchors.topMargin: Metrics.gap14
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        radius: Metrics.radiusControl
        color: Theme.color.surfaceWindow
        border.width: Metrics.hairline
        border.color: Theme.color.strokeHairline
        clip: true
        ListView {
            objectName: "developerLogList"
            anchors.fill: parent
            anchors.margins: Metrics.hairline
            model: log.rows
            boundsBehavior: Flickable.StopAtBounds
            onCountChanged: if (log.autoTail) positionViewAtEnd()
            ScrollBar.vertical: ViewportScrollBar {}
            delegate: Item {
                id: record
                required property var modelData
                width: ListView.view.width
                // The row follows the wrapped message, so a long line is not cut.
                height: Math.max(message.implicitHeight, Metrics.gap14) + Metrics.gap6
                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: Metrics.gap12
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap12
                    anchors.top: parent.top
                    anchors.topMargin: Metrics.gap2 + Metrics.hairline // derived: QBZ's 3
                    spacing: Metrics.gap10
                    Text {
                        width: log.colTime
                        text: Qt.formatDateTime(new Date(Number(record.modelData.ts || 0)), "yyyy-MM-dd hh:mm:ss.zzz")
                        color: Theme.color.textTertiary
                        font.pointSize: Type.label
                        elide: Text.ElideRight
                    }
                    Text {
                        readonly property string name: String(record.modelData.level || "").toUpperCase()
                        width: log.colLevel
                        text: name
                        font.pointSize: Type.label
                        font.weight: Type.semibold
                        // Only the level carries its color; the message stays plain.
                        color: name === "ERROR" ? Theme.color.statusNegative : name === "WARN" ? Theme.color.statusWarning
                            : name === "DEBUG" || name === "TRACE" ? Theme.color.textTertiary : Theme.color.textSecondary
                    }
                    Text { width: log.colTarget; text: record.modelData.target || ""; color: Theme.color.textTertiary; font.pointSize: Type.label; elide: Text.ElideRight }
                    Text {
                        id: message
                        width: parent.width - log.colTime - log.colLevel - log.colTarget - Metrics.gap10 * 3 // derived: three gaps
                        text: record.modelData.message || ""
                        color: Theme.color.textPrimary
                        font.pointSize: Type.label
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: log.rows.length === 0
            text: I18n.tr("No log entries.", I18n.rev)
            color: Theme.color.textTertiary
            font.pointSize: Type.body
        }
    }
}
