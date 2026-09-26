import QtQuick
import "../Theme"

// A level from 0 to 100 on a thin track (design/02 7, the player's volume):
// a click goes there, a drag follows the pointer, the wheel steps by five.
// The fill turns to the accent and a knob shows under the pointer.
Item {
    id: slider

    property int value: 0
    property string tip

    signal moved(int value)

    readonly property bool active: area.containsMouse || area.pressed

    implicitWidth: Metrics.volumeWidth
    implicitHeight: Metrics.button28

    Rectangle {
        id: track
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        height: Metrics.volumeTrack
        radius: height / 2 // derived: round ends
        color: Theme.color.strokeHairline

        Rectangle {
            width: track.width * slider.value / 100 // derived: the level's share
            height: parent.height
            radius: parent.radius
            color: slider.active ? Theme.color.accent : Theme.color.textSecondary
        }
    }

    Rectangle {
        visible: slider.active && slider.enabled
        anchors.verticalCenter: parent.verticalCenter
        x: track.width * slider.value / 100 - width / 2 // derived: centered on the level
        width: Metrics.seekKnob
        height: Metrics.seekKnob
        radius: width / 2 // derived: a circle
        color: Theme.color.accent
    }

    MouseArea {
        id: area

        anchors.fill: parent
        hoverEnabled: true

        function at(x) {
            return Math.round(Math.max(0, Math.min(1, x / width)) * 100) // derived: a share as a level
        }

        onPressed: (mouse) => slider.moved(at(mouse.x))
        onPositionChanged: (mouse) => {
            if (pressed)
                slider.moved(at(mouse.x))
        }
        onWheel: (wheel) => slider.moved(Math.max(0, Math.min(100, slider.value + (wheel.angleDelta.y > 0 ? Metrics.volumeStep : -Metrics.volumeStep))))
        onContainsMouseChanged: containsMouse && slider.tip !== "" ? TipState.show(slider.tip, slider) : TipState.hide(slider)
    }
}
