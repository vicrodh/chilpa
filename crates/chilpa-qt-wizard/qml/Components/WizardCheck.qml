import QtQuick
import app.chilpa.kit
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Step 1: the audio stack's verdict, the distro and the init (detected,
// always changeable), and one command to paste per missing piece. In a
// sandbox there is no verdict, only reference commands.
Column {
    id: step
    property var doc: ({})
    spacing: Metrics.gap14

    Text {
        width: parent.width
        text: I18n.tr("Check Audio Stack", I18n.rev)
        color: Theme.color.textPrimary
        font.pointSize: Type.section
        font.weight: Type.semibold
    }
    WizardBanner {
        visible: step.doc.sandboxed === true
        variant: "info"
        title: I18n.tr("Sandboxed — host check unavailable", I18n.rev)
        body: I18n.tr("Chilpa is packaged as Flatpak/Snap and can't inspect your host audio stack. If playback already works, you're set. Pick your distribution and init system below for the right setup commands.", I18n.rev)
    }
    WizardBanner {
        objectName: "wizardReady"
        visible: step.doc.sandboxed !== true && step.doc.healthOk === true
        variant: "success"
        title: I18n.tr("Audio stack ready", I18n.rev)
        body: step.doc.healthSummary || ""
    }
    WizardBanner {
        objectName: "wizardMissing"
        visible: step.doc.sandboxed !== true && step.doc.healthOk !== true
        variant: "warning"
        title: I18n.tr("Some packages are missing", I18n.rev)
        body: step.doc.healthSummary || ""
    }
    Repeater {
        model: [
            { label: I18n.tr("Your distribution", I18n.rev), options: step.doc.distroOptions || [], index: step.doc.distroIndex || 0, action: "setDistro" },
            { label: I18n.tr("Init system (service commands)", I18n.rev), options: step.doc.initOptions || [], index: step.doc.initIndex || 0, action: "setInit" }
        ]
        delegate: Column {
            required property var modelData
            width: step.width
            spacing: Metrics.gap8
            Text {
                text: modelData.label
                color: Theme.color.textSecondary
                font.pointSize: Type.small
                font.weight: Type.medium
            }
            SelectField {
                width: parent.width
                options: modelData.options
                values: modelData.options.map((_, i) => String(i))
                value: String(modelData.index)
                onPicked: (value) => HifiWizard.command(modelData.action, value)
            }
        }
    }
    Repeater {
        model: step.doc.remediations || []
        delegate: CommandBlock {
            required property var modelData
            caption: modelData.caption || ""
            command: modelData.command || ""
        }
    }
}
