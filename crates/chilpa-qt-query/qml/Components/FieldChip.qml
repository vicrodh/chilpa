import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A field to add a condition on (F2): 26 tall, radius 13, a hairline ring;
// the one whose values are open is inverted. "Fields" and "Group" take the
// same shape without a fill.
Rectangle {
    id: chip
    property string glyph
    property string text
    property bool active: false
    property bool quiet: false
    signal clicked()

    implicitWidth: line.implicitWidth + Refine.fieldChipPad * 2 // derived: padding on both sides
    implicitHeight: Refine.fieldChipTall
    radius: Refine.fieldChipRound
    color: active ? Theme.color.textPrimary : quiet ? "transparent" : Theme.color.surfaceWindow
    border.width: active ? 0 : Metrics.hairline
    border.color: Theme.color.strokeHairline

    Row {
        id: line
        anchors.centerIn: parent
        spacing: Refine.fieldChipApart
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: chip.glyph
            size: Refine.fieldChipIcon
            color: chip.active ? Theme.color.surfaceWindow : Theme.color.textSecondary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: chip.text
            font.pointSize: Type.meta
            color: chip.active ? Theme.color.surfaceWindow : Theme.color.textSecondary
        }
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: chip.clicked()
    }
}
