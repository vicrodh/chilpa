import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// One condition on the crumb line while Refine is closed (F1): its field's
// glyph (a minus when it keeps things out, brackets for a group), the field
// and the values, and an x. A click opens Refine.
Rectangle {
    id: chip
    // A row of the facade's reading, or a group.
    property var row: ({})
    // For a query too deep for rows: its count of conditions.
    property bool asText: false
    signal opened()
    signal removed()

    implicitWidth: line.implicitWidth + Refine.chipPadLeft + Refine.chipPadRight
    implicitHeight: Refine.chipTall
    radius: Refine.chipRound
    color: Theme.color.stateSelected

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: chip.opened()
    }
    Row {
        id: line
        x: Refine.chipPadLeft
        anchors.verticalCenter: parent.verticalCenter
        spacing: Refine.chipApart
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: chip.asText ? Icons.terminal2 : chip.row.group ? Icons.brackets : chip.row.negated ? Icons.minus : RefineText.glyph(chip.row.field)
            size: Refine.chipIcon
            color: chip.row.negated ? Theme.color.textSecondary : Theme.color.textTertiary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: chip.asText ? I18n.tr("Refine", I18n.rev) : RefineText.key(chip.row)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Text {
            objectName: "summaryChipValue"
            anchors.verticalCenter: parent.verticalCenter
            text: chip.asText ? I18n.trCount("{} condition", "{} conditions", chip.row.count || 0, I18n.rev) : RefineText.summary(chip.row)
            font.pointSize: Type.meta
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        RemoveDot {
            objectName: "summaryChipRemove"
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Remove", I18n.rev)
            onClicked: chip.removed()
        }
    }
}
