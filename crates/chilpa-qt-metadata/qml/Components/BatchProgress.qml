import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Rectangle {
    id: root
    objectName: "batchGlobalProgress"
    property var progress: ({})
    readonly property string phase: progress.phase || "idle"
    readonly property bool running: ["opening", "working", "saving", "finalizing"].includes(phase)
    readonly property bool determinate: progress.totalKnown === true && Number(progress.total) > 0 && phase !== "finalizing"
    readonly property real fraction: determinate ? Math.max(0, Math.min(1, Number(progress.completed || 0) / Number(progress.total))) : phase === "complete" ? 1 : 0
    visible: Number(progress.operationId || 0) > 0 && phase !== "idle"
    implicitHeight: visible ? body.implicitHeight + Metrics.gap12 * 2 : 0 // derived: padding above and below
    radius: Metrics.radiusControl
    color: Theme.color.fillField
    function title() {
        switch (phase) {
        case "opening": return I18n.tr("Reading folder metadata…", I18n.rev)
        case "working": return I18n.tr("Preparing metadata changes…", I18n.rev)
        case "saving": return I18n.tr("Writing reviewed changes…", I18n.rev)
        case "finalizing": return I18n.tr("Updating the library…", I18n.rev)
        case "cancelled": return I18n.tr("Operation cancelled", I18n.rev)
        case "failed": return I18n.tr("Operation stopped with errors", I18n.rev)
        default: return I18n.tr("Operation complete", I18n.rev)
        }
    }
    Column {
        id: body
        x: Metrics.gap12; y: Metrics.gap12
        width: parent.width - Metrics.gap12 * 2 // derived: horizontal padding
        spacing: Metrics.gap6
        Row {
            width: parent.width; spacing: Metrics.gap8
            BusyIndicator {
                width: Metrics.iconSm; height: width
                running: root.visible && root.running && !root.determinate
                visible: running
            }
            Text {
                objectName: "batchProgressTitle"
                width: parent.width - (root.running && !root.determinate ? Metrics.iconSm + parent.spacing : 0)
                text: root.title(); textFormat: Text.PlainText; wrapMode: Text.WordWrap
                font.pointSize: Type.body; font.weight: Type.medium
                color: root.phase === "failed" ? Theme.color.statusNegative : Theme.color.textPrimary
            }
        }
        ProgressBar {
            objectName: "batchProgressBar"
            width: parent.width; implicitHeight: Metrics.progress
            from: 0; to: 1; value: root.fraction
            Accessible.name: root.title()
            background: Rectangle { color: Theme.color.strokeHairline; radius: height }
            contentItem: Item {
                Rectangle { width: parent.width * root.fraction; height: parent.height; radius: height; color: Theme.color.accent }
            }
        }
        Text {
            objectName: "batchProgressCounts"
            width: parent.width; visible: text !== ""
            text: root.progress.totalKnown === true
                ? I18n.trArgs("{} of {} files processed", [String(root.progress.completed || 0), String(root.progress.total || 0)], I18n.rev)
                : root.phase === "opening" ? I18n.trCount("{} file read", "{} files read", root.progress.completed || 0, I18n.rev) : ""
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.meta; color: Theme.color.textSecondary
        }
        Text {
            width: parent.width; visible: Number(root.progress.saved || 0) + Number(root.progress.failed || 0) > 0
            text: I18n.trArgs("{} saved · {} failed", [String(root.progress.saved || 0), String(root.progress.failed || 0)], I18n.rev)
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.meta; color: Number(root.progress.failed || 0) > 0 ? Theme.color.statusWarning : Theme.color.textSecondary
        }
    }
}
