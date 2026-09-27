import QtQuick
import "../Theme"

// A horizontal or vertical level from 0 to 100 on a thin track: a click goes
// there, a drag follows the pointer, and the wheel steps by five.
Item {
    id: slider

    property int value: 0
    property string tip
    property bool vertical: false

    signal moved(int value)

    readonly property bool active: area.containsMouse || area.pressed

    implicitWidth: vertical ? Metrics.button28 : Metrics.volumeWidth
    implicitHeight: vertical ? Metrics.volumeVerticalLength : Metrics.button28

    Rectangle {
        id: track
        anchors.horizontalCenter: slider.vertical ? parent.horizontalCenter : undefined
        anchors.verticalCenter: slider.vertical ? undefined : parent.verticalCenter
        width: slider.vertical ? Metrics.volumeTrack : parent.width
        height: slider.vertical ? parent.height : Metrics.volumeTrack
        radius: Math.min(width, height) / 2 // derived: round ends
        color: Theme.color.strokeHairline

        Rectangle {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            width: slider.vertical ? parent.width : track.width * slider.value / 100 // derived: the level's share
            height: slider.vertical ? track.height * slider.value / 100 : parent.height // derived: the level's share
            radius: parent.radius
            color: slider.active ? Theme.color.accent : Theme.color.textSecondary
        }
    }

    Rectangle {
        visible: slider.active && slider.enabled
        anchors.horizontalCenter: slider.vertical ? parent.horizontalCenter : undefined
        anchors.verticalCenter: slider.vertical ? undefined : parent.verticalCenter
        x: slider.vertical ? 0 : track.width * slider.value / 100 - width / 2 // derived: centered on the level
        y: slider.vertical ? track.height * (1 - slider.value / 100) - height / 2 : 0 // derived: bottom is silence
        width: Metrics.seekKnob
        height: Metrics.seekKnob
        radius: width / 2 // derived: a circle
        color: Theme.color.accent
    }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor

        anchors.fill: parent
        hoverEnabled: true

        function at(x, y) {
            let share = slider.vertical ? 1 - y / height : x / width
            return Math.round(Math.max(0, Math.min(1, share)) * 100) // derived: a share as a level
        }

        onPressed: (mouse) => slider.moved(at(mouse.x, mouse.y))
        onPositionChanged: (mouse) => {
            if (pressed)
                slider.moved(at(mouse.x, mouse.y))
        }
        onWheel: (wheel) => slider.moved(Math.max(0, Math.min(100, slider.value + (wheel.angleDelta.y > 0 ? Metrics.volumeStep : -Metrics.volumeStep))))
        onContainsMouseChanged: containsMouse && slider.tip !== "" ? TipState.show(slider.tip, slider) : TipState.hide(slider)
    }
}
