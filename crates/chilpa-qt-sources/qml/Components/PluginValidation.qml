import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Column {
    id: status
    property string phase: "pending"
    property string problem: ""
    spacing: Metrics.gap6
    readonly property color light: phase === "passed" || phase === "ready" ? Theme.color.statusPositive
        : phase === "failed" ? Theme.color.statusNegative : phase === "running" ? Theme.color.statusWarning : Theme.color.textTertiary
    Row {
        width: parent.width
        spacing: Metrics.gap8
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.gap12
            height: width
            radius: width / 2 // derived: circular status indicator
            color: status.light
        }
        Text {
            objectName: "validationLabel"
            width: parent.width - Metrics.gap12 - Metrics.gap8
            text: status.phase === "running" ? I18n.tr("Inspecting plugin…", I18n.rev)
                : status.phase === "passed" ? I18n.tr("Inspection passed", I18n.rev)
                : status.phase === "failed" ? I18n.tr("Inspection failed", I18n.rev)
                : status.phase === "ready" ? I18n.tr("Plugin initialized successfully", I18n.rev)
                : I18n.tr("Not inspected", I18n.rev)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: status.light
        }
    }
    Text {
        width: parent.width
        text: status.phase === "passed" ? I18n.tr("Review the requested permissions before installing.", I18n.rev) : status.problem
        visible: text !== ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
