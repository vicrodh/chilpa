import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The volume of the immersive bar: a 2 px track with its fill, a 12 px
// thumb that appears on hover, and a bubble with the percentage above.
// Locked (the output owns its volume), everything is inert and dimmer.
Item {
    id: bar
    property real value: 0
    property bool locked: false
    signal changed(real v)
    signal released(real v)
    readonly property real shown: Math.max(0, Math.min(1, value))
    readonly property bool hot: !locked && (area.containsMouse || area.pressed)
    function clamped(v) {
        return Math.max(0, Math.min(1, v))
    }
    height: Stage.volumeThumb
    Rectangle {
        x: bar.width - width
        y: -Stage.bubbleUp
        width: Stage.bubbleWide
        height: Stage.bubbleTall
        radius: Stage.bubbleRound
        color: Theme.color.immersiveBubble
        visible: bar.hot
        Text {
            anchors.fill: parent
            text: Math.round(bar.shown * 100) // derived: a percentage
            color: Theme.color.immersiveTextStrong
            font.pointSize: Stage.bubbleText
            font.weight: Type.semibold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
    Rectangle {
        y: (parent.height - height) / 2 // derived: centered on the thumb
        width: bar.width
        height: Stage.volumeTrack
        radius: Stage.tinyRound
        color: bar.locked ? Theme.color.immersiveVolumeTrackLocked : Theme.color.immersiveVolumeTrack
    }
    Rectangle {
        y: (parent.height - height) / 2 // derived: centered on the thumb
        width: bar.width * bar.shown
        height: Stage.volumeTrack
        radius: Stage.tinyRound
        color: bar.locked ? Theme.color.immersiveVolumeFillLocked : Theme.color.immersiveVolumeFill
    }
    Rectangle {
        x: bar.width * bar.shown - width / 2 // derived: centered on the fill's end
        width: Stage.volumeThumb
        height: Stage.volumeThumb
        radius: width / 2 // derived: a circle
        color: Theme.color.immersiveTextStrong
        opacity: bar.hot ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.volumeThumbFade } }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: bar.locked ? Qt.ArrowCursor : Qt.PointingHandCursor
        onPressed: (mouse) => { if (!bar.locked) bar.changed(bar.clamped(mouse.x / width)) }
        onPositionChanged: (mouse) => { if (!bar.locked && pressed) bar.changed(bar.clamped(mouse.x / width)) }
        onReleased: (mouse) => { if (!bar.locked) bar.released(bar.clamped(mouse.x / width)) }
    }
}
