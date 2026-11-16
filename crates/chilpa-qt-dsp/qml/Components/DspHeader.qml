import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Row {
    id: header
    property bool processingEnabled: false
    property bool available: false
    signal enabledChangedByUser(bool enabled)
    signal dismissed()
    spacing: Metrics.gap20
    height: Math.max(titleBlock.height, enabledGroup.height, done.height)
    Column {
        id: titleBlock
        width: Math.max(0, header.width - enabledGroup.width - done.width - header.spacing * 2) // derived: title beside the switch and close action
        spacing: Metrics.gap4
        Text {
            width: parent.width
            text: I18n.tr("Chilpa DSP", I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.section
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: I18n.tr("Audio processing", I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
    Row {
        id: enabledGroup
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap10
        Text {
            visible: header.width >= Metrics.dspCompactWidth
            anchors.verticalCenter: parent.verticalCenter
            text: header.processingEnabled ? I18n.tr("DSP enabled", I18n.rev) : I18n.tr("Bypass", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Toggle {
            objectName: "dspEnabled"
            checked: header.processingEnabled
            enabled: header.available
            Accessible.name: I18n.tr("Enable audio processing; turn off for bypass", I18n.rev)
            onToggled: on => header.enabledChangedByUser(on)
        }
    }
    PillButton {
        id: done
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Done", I18n.rev)
        onClicked: header.dismissed()
    }
}
