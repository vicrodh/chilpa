import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: help
    property var lines: []
    property real room: 0
    color: Theme.color.surfaceRaised
    border.color: Theme.color.strokeHairline
    height: Math.min(room, heading.implicitHeight + linesView.contentHeight + Metrics.gap8 * 3) // derived: heading gap and vertical padding
    Column {
        id: content
        anchors.fill: parent
        anchors.margins: Metrics.gap8
        spacing: Metrics.gap8
        Text {
            id: heading
            width: parent.width
            wrapMode: Text.WordWrap
            text: I18n.tr("Command help · :help command for details · Esc to close", I18n.rev)
            color: Theme.color.textPrimary; font.pointSize: Type.meta; font.family: Type.consoleFamily
        }
        ListView {
            id: linesView
            objectName: "commandHelpLines"
            width: parent.width
            implicitHeight: contentHeight
            height: Math.max(0, help.height - y - Metrics.gap8)
            model: help.lines
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar {}
            delegate: Text {
                required property var modelData
                width: ListView.view.width - Metrics.gap12
                // An example sits under its command, indented, and reads brighter.
                text: (modelData.example ? "  " : "") + modelData.usage + "  " + modelData.description
                wrapMode: Text.WordWrap
                color: modelData.example ? Theme.color.textPrimary : Theme.color.textSecondary
                font.pointSize: Type.body; font.family: Type.consoleFamily
            }
        }
    }
}
