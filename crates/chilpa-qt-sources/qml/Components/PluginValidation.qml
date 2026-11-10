import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

StatusArea {
    id: status
    property string phase: "pending"
    property string problem: ""
    property bool inspection: true
    readonly property color light: phase === "passed" || phase === "ready" ? Theme.color.statusPositive
        : phase === "failed" ? Theme.color.statusNegative : phase === "running" ? Theme.color.statusWarning : Theme.color.textTertiary
    labelColor: light
    labelObjectName: "validationLabel"
    label: inspection ? phase === "running" ? I18n.tr("Inspecting plugin…", I18n.rev)
        : phase === "passed" ? I18n.tr("Inspection passed", I18n.rev)
        : phase === "failed" ? I18n.tr("Inspection failed", I18n.rev)
        : phase === "ready" ? I18n.tr("Plugin initialized successfully", I18n.rev)
        : I18n.tr("Not inspected", I18n.rev) : ""
    text: inspection && phase === "passed" && problem === "" ? I18n.tr("Review the requested permissions before installing.", I18n.rev) : problem
}
