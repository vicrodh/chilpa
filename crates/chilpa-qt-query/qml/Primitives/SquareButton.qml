import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The panel's square icon button: 28 in its head, 24 inside rows, radius 7,
// quiet until hovered, lit when on (the console toggle). Always a tooltip.
Item {
    id: button
    required property string tip
    property string glyph
    property bool small: false
    property bool checked: false
    // Off but still shown, as Undo with nothing to undo.
    property bool quiet: false
    // The glyph's own colour when it is neither on nor quiet.
    property color tint: Theme.color.textSecondary
    signal clicked()

    implicitWidth: small ? Refine.rowButton : Refine.button
    implicitHeight: implicitWidth

    Rectangle {
        anchors.fill: parent
        radius: Refine.buttonRound
        color: button.checked ? Theme.color.stateSelected : area.containsMouse && !button.quiet ? Theme.color.stateHover : "transparent"
    }
    Icon {
        anchors.centerIn: parent
        glyph: button.glyph
        size: button.small ? Refine.rowButtonIcon : Refine.buttonIcon
        color: button.checked ? Theme.color.accent : button.quiet ? Theme.color.strokeHairline : button.tint
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: if (!button.quiet) button.clicked()
        onContainsMouseChanged: containsMouse && button.tip !== "" ? TipState.show(button.tip, button) : TipState.hide(button)
    }
    Component.onDestruction: TipState.hide(button)
}
