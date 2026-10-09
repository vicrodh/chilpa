import QtQuick
import QtQuick.Effects
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The play button of the immersive bar: a 48 px white disc over a blurred
// shadow, a black glyph of 20 px nudged one pixel right while it offers
// play, so the triangle reads centered.
Item {
    id: disc
    property bool playing: false
    property bool shaders: true
    signal clicked()
    width: Stage.playDisc
    height: Stage.playDisc
    Rectangle {
        anchors.fill: parent
        y: Stage.playShadowDrop
        radius: width / 2 // derived: a circle
        color: Theme.color.immersiveShadow
        layer.enabled: disc.shaders
        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachPlay; blur: 0.5 }
    }
    Rectangle {
        anchors.fill: parent
        radius: width / 2 // derived: a circle
        color: area.containsMouse ? Theme.color.immersivePlayDiscHover : Theme.color.immersivePlayDisc
        Text {
            x: Math.round((parent.width - width) / 2) + (disc.playing ? 0 : 1) // derived: centered, a pixel right for the triangle
            y: Math.round((parent.height - height) / 2) // derived: centered
            text: disc.playing ? Icons.playerPauseFilled : Icons.playerPlayFilled
            font.family: Icons.filled
            font.pixelSize: Stage.playGlyph
            color: Theme.color.immersivePlayGlyph
        }
        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: disc.clicked()
        }
    }
}
