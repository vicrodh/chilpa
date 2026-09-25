import QtQuick
import "../Theme"

// A button with text: "primary" in the accent, "secondary" on a field.
Rectangle {
    id: root

    property string text
    property string glyph
    // "primary" or "secondary".
    property string kind: "secondary"

    signal clicked()

    readonly property bool primary: kind === "primary"

    implicitWidth: row.implicitWidth + Metrics.pillPadding * 2 // derived: padding on both sides
    implicitHeight: Metrics.pillHeight
    radius: height / 2 // derived: a pill
    color: !primary ? (area.containsMouse ? Theme.color.stateSelected : Theme.color.fillField)
         : area.pressed ? Theme.color.accentPressed
         : area.containsMouse ? Theme.color.accentHover : Theme.color.accent

    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.glyph !== ""
            glyph: root.glyph
            filled: root.primary
            size: Metrics.iconSm
            color: root.primary ? Theme.color.textOnAccent : Theme.color.textPrimary
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            font.pointSize: Type.body
            font.weight: root.primary ? Type.semibold : Type.medium
            color: root.primary ? Theme.color.textOnAccent : Theme.color.textPrimary
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.clicked()
    }
}
