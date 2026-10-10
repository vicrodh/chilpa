import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// QBZ's CommandBlock: what a command is for, the command in monospace and a
// button that copies it, with a check for two seconds after.
Rectangle {
    id: block
    property string caption
    property string command
    property bool copied: false
    width: parent ? parent.width : 0
    implicitHeight: column.implicitHeight + Metrics.gap24
    radius: Metrics.radiusControl
    color: Theme.color.surfaceRaised
    border.width: 1
    border.color: Theme.color.strokeHairline

    Clipboard { id: clipboard }
    Timer { id: settle; interval: WizardFrame.copiedMs; onTriggered: block.copied = false }

    Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap8
        Text {
            visible: block.caption !== ""
            width: parent.width
            text: block.caption
            color: Theme.color.textSecondary
            font.pointSize: Type.small
            font.weight: Type.medium
            wrapMode: Text.WordWrap
        }
        Item {
            width: parent.width
            height: Math.max(line.implicitHeight, WizardFrame.copyButton)
            Text {
                id: line
                objectName: "commandText"
                anchors.left: parent.left
                anchors.right: copy.left
                anchors.rightMargin: Metrics.gap10
                text: block.command
                color: Theme.color.textPrimary
                font.family: "monospace"
                font.pointSize: Type.small
                wrapMode: Text.WrapAnywhere
            }
            IconButton {
                id: copy
                objectName: "commandCopy"
                anchors.right: parent.right
                size: WizardFrame.copyButton
                glyph: block.copied ? Lucide.check : Lucide.copy
                iconFamily: Lucide.font
                tip: I18n.tr("Copy", I18n.rev)
                onClicked: {
                    clipboard.write(block.command)
                    block.copied = true
                    settle.restart()
                }
            }
        }
    }
}
