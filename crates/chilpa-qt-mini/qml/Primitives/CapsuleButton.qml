import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// A button of the mini player's capsule (02-mini §1.5, QBZ's CapBtn):
// 22 by 20 with soft corners, never a disc; the capsule's trigger makes it
// square. `clicked` is the ordinary press and release; `pressed` fires on
// the way down, for the button that hands the window to the desktop to
// move, since that grab never lets a release come back. `danger` is the
// close button's red fill with a white glyph on it.
Rectangle {
    id: button

    property string glyph
    property int iconSize: Card.capsuleIcon
    property bool active: false
    property real idleOpacity: Card.idleCapsuleGlyph
    property bool danger: false
    property bool dragCursor: false
    readonly property bool hovered: area.containsMouse

    signal clicked()
    signal pressed()

    width: Card.capsuleButton
    height: Card.capsuleButtonTall
    radius: Card.capsuleButtonRound
    antialiasing: true
    color: area.containsMouse ? (button.danger ? Theme.color.miniDanger : Theme.color.miniCapsuleHover) : "transparent"

    Text {
        text: button.glyph
        font.family: Icons.outline
        font.pixelSize: button.iconSize
        width: button.iconSize
        height: button.iconSize
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        x: Math.round((button.width - width) / 2) // derived: centered
        y: Math.round((button.height - height) / 2) // derived: centered
        color: button.active ? Theme.color.accent : area.containsMouse && button.danger ? Theme.color.miniGlyphOnBackdrop : Theme.color.textPrimary
        opacity: button.active || area.containsMouse ? 1 : button.idleOpacity
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: button.dragCursor ? Qt.OpenHandCursor : Qt.PointingHandCursor
        onClicked: button.clicked()
        onPressed: button.pressed()
    }
}
