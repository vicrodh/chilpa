import QtQuick
import app.chilpa.kit
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Step 2: the sinks found, the ones that look like a DAC checked, each with
// its bus, whether it is the default and the rates its card lists. "Can't
// see your DAC?" opens the manual entry of a node name, which is open from
// the start when nothing was found.
Column {
    id: step
    property var doc: ({})
    property bool manual: false
    signal manualToggled()
    spacing: Metrics.gap14

    Text {
        width: parent.width
        text: I18n.tr("Which DACs do you want to configure?", I18n.rev)
        color: Theme.color.textPrimary
        font.pointSize: Type.section
        font.weight: Type.semibold
        wrapMode: Text.WordWrap
    }
    Text {
        visible: step.doc.detecting === true
        text: I18n.tr("Detecting your DACs…", I18n.rev)
        color: Theme.color.textTertiary
        font.pointSize: Type.body
    }
    Repeater {
        model: step.doc.candidates || []
        delegate: Rectangle {
            required property int index
            required property var modelData
            width: step.width
            height: candidate.implicitHeight + Metrics.gap16
            radius: Metrics.radiusControl
            color: modelData.checked ? Theme.color.stateHover : "transparent"
            CircleCheck {
                id: check
                objectName: "wizardDac" + index
                anchors.left: parent.left
                anchors.leftMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                checkable: false
                checked: modelData.checked === true
                onClicked: HifiWizard.command("toggleDac", String(index))
            }
            Column {
                id: candidate
                anchors.left: check.right
                anchors.leftMargin: Metrics.gap4
                anchors.right: parent.right
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap4
                Text {
                    width: parent.width
                    text: modelData.description || ""
                    color: Theme.color.textPrimary
                    font.pointSize: Type.body
                    font.weight: Type.medium
                    elide: Text.ElideRight
                }
                Row {
                    spacing: Metrics.gap8
                    Rectangle {
                        visible: (modelData.bus || "") !== ""
                        width: bus.implicitWidth + Metrics.gap12
                        height: WizardFrame.busBox
                        radius: Metrics.radiusItem
                        color: Theme.color.surfaceRaised
                        border.width: 1
                        border.color: Theme.color.strokeHairline
                        Text {
                            id: bus
                            anchors.centerIn: parent
                            text: modelData.bus || ""
                            color: Theme.color.textTertiary
                            font.pointSize: Type.micro
                            font.weight: Type.semibold
                        }
                    }
                    Text {
                        visible: modelData.isDefault === true
                        text: I18n.tr("default", I18n.rev)
                        color: Theme.color.statusPositive
                        font.pointSize: Type.small
                    }
                    Text {
                        visible: (modelData.ratesLabel || "") !== ""
                        text: modelData.ratesLabel || ""
                        color: Theme.color.textTertiary
                        font.pointSize: Type.small
                    }
                }
            }
        }
    }
    Text {
        objectName: "wizardManualToggle"
        visible: step.doc.hasEnumeration === true
        text: step.manual ? I18n.tr("Hide manual entry", I18n.rev) : I18n.tr("Can't see your DAC?", I18n.rev)
        color: Theme.color.accent
        font.pointSize: Type.small
        TapHandler { onTapped: step.manualToggled() }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
    Column {
        objectName: "wizardManual"
        visible: step.manual || step.doc.hasEnumeration !== true
        width: parent.width
        spacing: Metrics.gap10
        Text {
            width: parent.width
            text: I18n.tr("Find your DAC's PipeWire node name and paste it here:", I18n.rev)
            color: Theme.color.textSecondary
            font.pointSize: Type.small
            wrapMode: Text.WordWrap
        }
        CommandBlock {
            caption: I18n.tr("1. Find your DAC under 'Audio > Sinks' and note its number", I18n.rev)
            command: "wpctl status"
        }
        CommandBlock {
            caption: I18n.tr("2. Run with that number to read the node.name", I18n.rev)
            command: "wpctl inspect <ID> | grep node.name"
        }
        TextField {
            id: field
            objectName: "wizardManualField"
            width: parent.width
            placeholder: "alsa_output.usb-..."
            onTextEdited: HifiWizard.command("validateManual", text)
        }
        Text {
            visible: field.text !== ""
            width: parent.width
            text: step.doc.manualValid === true ? I18n.tr("Valid node name", I18n.rev)
                                                 : I18n.tr("Invalid — should contain alsa_output or alsa_input", I18n.rev)
            color: step.doc.manualValid === true ? Theme.color.statusPositive : Theme.color.statusNegative
            font.pointSize: Type.small
            wrapMode: Text.WordWrap
        }
    }
    function clear() { field.text = "" }
}
