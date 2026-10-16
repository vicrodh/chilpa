import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The thin seek of the immersive bar and of the cinematic card: a 2 px
// track at 18 % white, a fill of the given color, and a 10 px grab band
// that seeks on press and on drag.
Item {
    id: bar
    property real value: 0
    property color fill: Theme.color.immersiveTextStrong
    signal changed(real v)
    function clamped(v) {
        return Math.max(0, Math.min(1, v))
    }
    height: Stage.tinyGrab
    Rectangle {
        y: (parent.height - height) / 2 // derived: centered in the grab band
        width: bar.width
        height: Stage.tinyTrack
        radius: Stage.tinyRound
        color: Theme.color.immersiveSeekTrack
    }
    Rectangle {
        y: (parent.height - height) / 2 // derived: centered in the grab band
        width: bar.width * bar.clamped(bar.value)
        height: Stage.tinyTrack
        radius: Stage.tinyRound
        color: bar.fill
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onPressed: (mouse) => bar.changed(bar.clamped(mouse.x / width))
        onPositionChanged: (mouse) => { if (pressed) bar.changed(bar.clamped(mouse.x / width)) }
    }
}
