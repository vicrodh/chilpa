import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Where Refine was while it is in its own window (F9): one quiet line and
// "Dock" to bring it back.
Rectangle {
    id: note
    signal dockAsked()
    implicitHeight: Refine.dockedTall
    radius: Refine.dockedRound
    color: "transparent"
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    Row {
        x: Refine.footSide
        anchors.verticalCenter: parent.verticalCenter
        spacing: Refine.headApart
        Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.appWindow; size: Refine.labelIcon; color: Theme.color.textSecondary }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Refine is in its own window", I18n.rev); font.pointSize: Type.row; color: Theme.color.textSecondary }
    }
    Rectangle {
        objectName: "dockButton"
        anchors.right: parent.right
        anchors.rightMargin: Refine.innerApart
        anchors.verticalCenter: parent.verticalCenter
        width: dock.implicitWidth + Refine.matchSegPad * 2 // derived: padding on both sides
        height: Refine.textButtonTall
        radius: Refine.buttonRound
        color: area.containsMouse ? Theme.color.stateHover : "transparent"
        Text { id: dock; anchors.centerIn: parent; text: I18n.tr("Dock", I18n.rev); font.pointSize: Type.row; color: Theme.color.textPrimary }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: note.dockAsked() }
    }
}
