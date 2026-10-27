import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: path
    objectName: "queueSignalPath"
    property var document: ({})
    readonly property var stages: document.stages || []
    readonly property color statusColor: document.status === "processed" ? Theme.color.accent : Theme.color.textSecondary
    spacing: Metrics.gap12

    Column {
        width: parent.width
        spacing: Metrics.gap4
        Text { text: I18n.tr("Audio path", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small }
        Text {
            id: statusText
            objectName: "queueSignalPathStatus"
            width: parent.width
            text: path.document.summary || ""
            textFormat: Text.PlainText
            color: path.statusColor
            font.pointSize: Type.meta
            font.weight: Type.medium
            wrapMode: Text.WordWrap
            HoverHandler { onHoveredChanged: hovered && path.document.detail ? TipState.show(path.document.detail, statusText) : TipState.hide(statusText) }
            Component.onDestruction: TipState.hide(statusText)
        }
    }
    Column {
        width: parent.width
        Repeater {
            model: path.stages
            delegate: Item {
                id: stage
                required property int index
                required property var modelData
                width: parent.width
                height: facts.height + Metrics.gap12
                Rectangle {
                    x: Metrics.gap6
                    y: Metrics.gap12
                    width: Metrics.hairline
                    height: parent.height - y
                    visible: stage.index < path.stages.length - 1
                    color: Theme.color.strokeHairline
                }
                Rectangle {
                    y: Metrics.gap4
                    width: Metrics.iconXs + Metrics.gap2
                    height: width
                    radius: width / 2 // derived: a circular marker
                    color: Theme.color.surfaceRaised
                    border.width: Metrics.hairline
                    border.color: Theme.color.strokeHairline
                    Rectangle { anchors.centerIn: parent; width: Metrics.gap4; height: width; radius: width / Metrics.gap2; color: path.statusColor }
                }
                Column {
                    id: facts
                    x: Metrics.gap24
                    width: parent.width - x
                    spacing: Metrics.gap2
                    Text { text: stage.modelData.label || ""; color: Theme.color.textTertiary; font.pointSize: Type.micro }
                    Text {
                        width: parent.width
                        text: stage.modelData.value || ""
                        textFormat: Text.PlainText
                        color: Theme.color.textPrimary
                        font.pointSize: Type.small
                        wrapMode: Text.WordWrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    Text { visible: text !== ""; width: parent.width; text: stage.modelData.mode || ""; color: Theme.color.textSecondary; font.pointSize: Type.micro; wrapMode: Text.WordWrap }
                }
                HoverHandler {
                    onHoveredChanged: {
                        const detail = stage.modelData.detail || ""
                        if (hovered && detail) TipState.show(detail, stage)
                        else TipState.hide(stage)
                    }
                }
                Component.onDestruction: TipState.hide(stage)
            }
        }
    }
}
