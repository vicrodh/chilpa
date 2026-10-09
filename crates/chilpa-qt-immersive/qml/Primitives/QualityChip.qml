import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// What plays, said in a pill over the dark field: the player's long
// quality line ("FLAC · 16-bit / 44.1 kHz") at 11 px with a hairline edge.
// Chilpa has no tier badge with a format mark, so this stands where QBZ's
// quality badge stood; it hides with nothing to say.
Rectangle {
    id: chip
    property string text
    property real scale: 1
    visible: text !== ""
    width: label.implicitWidth + Stage.chipPad * 2 * scale // derived: the text and its padding
    height: Stage.chipTall * scale
    radius: Stage.chipRound
    color: Theme.color.immersiveGlass
    border.width: Metrics.hairline
    border.color: Theme.color.immersiveGlassEdge
    Text {
        id: label
        anchors.centerIn: parent
        text: chip.text
        color: Theme.color.immersiveTextMuted
        font.pointSize: Stage.chipText * chip.scale
        font.weight: Type.medium
    }
}
