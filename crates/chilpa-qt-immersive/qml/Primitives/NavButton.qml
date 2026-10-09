import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Previous and next in the immersive bar: a 28 px circle filled at 15 %
// white, 25 % on hover, with a 14 px white glyph.
Rectangle {
    id: nav
    property string glyph
    property bool filled: true
    signal clicked()
    width: Stage.navSide
    height: Stage.navSide
    radius: width / 2 // derived: a circle
    color: area.containsMouse ? Theme.color.immersiveFill : Theme.color.immersiveNav
    Text {
        anchors.centerIn: parent
        text: nav.glyph
        font.family: nav.filled ? Icons.filled : Icons.outline
        font.pixelSize: Stage.ghostIcon + 2 // derived: the nav glyph is two pixels larger than a ghost's
        color: Theme.color.immersiveTextStrong
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: nav.clicked()
    }
}
