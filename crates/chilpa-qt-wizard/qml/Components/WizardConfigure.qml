import QtQuick
import app.chilpa.kit
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Step 3: the backup, each DAC's three files in an accordion (open when
// there is one DAC), and the restart, each with the box that says it was
// done. Chilpa writes none of it.
Column {
    id: step
    property var doc: ({})
    property bool backupDone: false
    property bool configDone: false
    property bool restartDone: false
    signal backupToggled()
    signal configToggled()
    signal restartToggled()
    spacing: Metrics.gap14

    Text {
        width: parent.width
        text: I18n.tr("Review & Apply", I18n.rev)
        color: Theme.color.textPrimary
        font.pointSize: Type.section
        font.weight: Type.semibold
    }
    WizardBanner {
        variant: "info"
        title: I18n.tr("Chilpa never writes these files", I18n.rev)
        body: I18n.tr("Chilpa already pins rate, sink and exclusive mode at runtime. These files only change system-wide behavior — copy and run them yourself if you want them. Back up first.", I18n.rev)
    }
    CommandBlock {
        caption: I18n.tr("1. Back up your current PipeWire/WirePlumber config", I18n.rev)
        command: step.doc.backupCmd || ""
    }
    CircleCheck {
        objectName: "wizardBackup"
        width: parent.width
        checkable: false
        checked: step.backupDone
        text: I18n.tr("I created a backup", I18n.rev)
        onClicked: step.backupToggled()
    }
    Repeater {
        model: step.doc.dacConfigs || []
        delegate: Column {
            id: config
            required property int index
            required property var modelData
            width: step.width
            spacing: Metrics.gap8
            Rectangle {
                width: parent.width
                height: WizardFrame.configHead
                radius: Metrics.radiusControl
                color: head.hovered ? Theme.color.stateHover : Theme.color.surfaceRaised
                border.width: 1
                border.color: Theme.color.strokeHairline
                Icon {
                    id: chevron
                    anchors.left: parent.left
                    anchors.leftMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: config.modelData.expanded ? Lucide.chevronDown : Lucide.chevronRight
                    iconFamily: Lucide.font
                    size: Metrics.iconSm
                    color: Theme.color.textTertiary
                }
                Text {
                    anchors.left: chevron.right
                    anchors.leftMargin: Metrics.gap8
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    text: config.modelData.name || ""
                    color: Theme.color.textPrimary
                    font.pointSize: Type.body
                    font.weight: Type.medium
                    elide: Text.ElideRight
                }
                HoverHandler { id: head; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: HifiWizard.command("toggleConfig", String(config.index)) }
            }
            Column {
                visible: config.modelData.expanded === true
                width: parent.width
                spacing: Metrics.gap8
                CommandBlock {
                    caption: I18n.tr("Sample-rate switching (PipeWire)", I18n.rev)
                    command: config.modelData.pipewireConf || ""
                }
                CommandBlock {
                    caption: I18n.tr("Per-app bit-perfect (PipeWire client)", I18n.rev)
                    command: config.modelData.pulseConf || ""
                }
                CommandBlock {
                    caption: I18n.tr("Pin the DAC + rates (WirePlumber)", I18n.rev)
                    command: config.modelData.wireplumberConf || ""
                }
            }
        }
    }
    CircleCheck {
        objectName: "wizardConfigured"
        width: parent.width
        checkable: false
        checked: step.configDone
        text: I18n.tr("I created the configuration files", I18n.rev)
        onClicked: step.configToggled()
    }
    CommandBlock {
        caption: I18n.tr("3. Restart the audio services to load the config", I18n.rev)
        command: step.doc.restartCmd || ""
    }
    CircleCheck {
        objectName: "wizardRestarted"
        width: parent.width
        checkable: false
        checked: step.restartDone
        text: I18n.tr("I restarted the audio services", I18n.rev)
        onClicked: step.restartToggled()
    }
}
