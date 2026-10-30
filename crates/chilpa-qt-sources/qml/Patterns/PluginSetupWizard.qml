import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A plugin supplies step labels and state; the host owns layout and styling.
Column {
    id: wizard
    required property var document
    readonly property var steps: Array.isArray(document.steps) ? document.steps.slice(0, 8) : []
    spacing: Metrics.gap24
    visible: steps.length > 0

    Row {
        width: parent.width
        spacing: Metrics.gap8
        Repeater {
            model: wizard.steps
            Column {
                id: step
                required property var modelData
                required property int index
                readonly property bool current: index + 1 === Number(wizard.document.step)
                readonly property bool complete: modelData.state === "complete"
                width: (wizard.width - (wizard.steps.length - 1) * Metrics.gap8) / Math.max(1, wizard.steps.length)
                spacing: Metrics.gap8
                Row {
                    width: parent.width
                    spacing: Metrics.gap8
                    Rectangle {
                        objectName: "setupStep_" + step.index
                        width: Metrics.fieldHeight
                        height: width
                        radius: width / 2 // derived: numbered circular step marker
                        color: step.current ? Theme.color.accent : step.complete ? Theme.color.surfaceRaised : Theme.color.fillField
                        border.width: step.current || step.complete ? 1 : 0
                        border.color: step.current ? Theme.color.accent : Theme.color.statusPositive
                        Text {
                            anchors.centerIn: parent
                            text: String(step.index + 1)
                            textFormat: Text.PlainText
                            font.pointSize: Type.body
                            font.weight: Type.semibold
                            color: step.current ? Theme.color.textOnAccent : step.complete ? Theme.color.statusPositive : Theme.color.textTertiary
                        }
                    }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(0, step.width - Metrics.fieldHeight - Metrics.gap8)
                        height: 1
                        visible: step.index + 1 < wizard.steps.length
                        color: step.complete ? Theme.color.statusPositive : Theme.color.strokeHairline
                    }
                }
                Text {
                    width: parent.width
                    text: step.modelData.label || ""
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    font.pointSize: Type.small
                    font.weight: step.current ? Type.semibold : Type.regular
                    color: step.current ? Theme.color.textPrimary : Theme.color.textSecondary
                }
            }
        }
    }
    Column {
        width: parent.width
        spacing: Metrics.gap8
        Text {
            width: parent.width
            text: wizard.document.eyebrow || ""
            visible: text !== ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.micro
            font.letterSpacing: Type.labelTracking
            color: Theme.color.textTertiary
        }
        Text {
            objectName: "setupWizardTitle"
            width: parent.width
            text: wizard.document.title || ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.section
            font.weight: Type.heading
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: wizard.document.description || ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            lineHeight: 1.25
            color: Theme.color.textSecondary
        }
    }
}
