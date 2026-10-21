import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A console query nested deeper than rows can show (F5): it says so, shows
// the query as text, still applies it, and offers to edit it as text.
Rectangle {
    id: block
    property string text
    signal editAsked()
    implicitHeight: column.implicitHeight + Refine.pickApart * 2 // derived: padding above and below
    radius: Refine.pickRound
    color: Theme.color.surfaceWindow
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    Column {
        id: column
        x: Refine.footSide
        y: Refine.pickApart
        width: parent.width - Refine.footSide * 2 // derived: padding on both sides
        spacing: Refine.headApart
        Row {
            width: parent.width
            spacing: Refine.headApart
            Icon { glyph: Icons.bracketsContain; size: Refine.buttonIcon; color: Theme.color.textSecondary }
            Text { width: parent.width - Refine.buttonIcon - Refine.headApart; text: I18n.tr("This query nests groups three levels deep. Rows show two, so it stays as text here. It still applies.", I18n.rev); font.pointSize: Type.row; color: Theme.color.textSecondary; wrapMode: Text.WordWrap }
        }
        Text { objectName: "complexText"; width: parent.width; text: block.text; font.family: Type.consoleFamily; font.pointSize: Type.row; color: Theme.color.textPrimary; wrapMode: Text.WrapAnywhere }
        Rectangle {
            objectName: "complexEdit"
            width: edit.implicitWidth + Refine.pillPad * 2 // derived: padding on both sides
            height: Refine.textButtonTall
            radius: Refine.pillRound
            color: Theme.color.fillField
            Row {
                id: edit
                anchors.centerIn: parent
                spacing: Refine.innerApart
                Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.pencil; size: Refine.rowButtonIcon; color: Theme.color.textPrimary }
                Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Edit as text", I18n.rev); font.pointSize: Type.row; font.weight: Type.medium; color: Theme.color.textPrimary }
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: block.editAsked() }
        }
    }
}
