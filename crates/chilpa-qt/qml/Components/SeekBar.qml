import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The progress of what plays, and where to take it (design/02 7): a thin
// line that thickens under the pointer, with a knob and the time under the
// pointer above it. A click goes there; a drag follows the pointer and goes
// where it is let go -- one seek, not one per pixel.
Item {
    id: bar

    property real position: 0
    property real duration: 0
    // The time at the pointer or the drag, as the owner writes it.
    property string bubbleText

    // The A-B loop's marks, in seconds; negative while not set.
    property real loopStart: -1
    property real loopEnd: -1

    signal seek(real seconds)

    readonly property bool active: area.containsMouse || area.pressed
    readonly property real shown: area.pressed ? dragged : position
    readonly property real hoverSeconds: secondsAt(area.mouseX)
    property real dragged: 0
    function secondsAt(x) { return duration * Math.max(0, Math.min(1, x / width)) }

    implicitHeight: Metrics.seekHit

    Rectangle {
        id: track
        anchors.bottom: parent.bottom
        width: parent.width
        height: bar.active ? Metrics.progressHover : Metrics.progress
        radius: height / 2 // derived: round ends
        color: Theme.color.strokeHairline

        Rectangle {
            width: bar.duration > 0 ? parent.width * Math.min(1, bar.shown / bar.duration) : 0
            height: parent.height
            radius: parent.radius
            color: bar.active ? Theme.color.accent : Theme.color.textSecondary
        }
        // The loop's stretch once it has an end, and a tick at each mark.
        Rectangle {
            visible: bar.loopEnd > bar.loopStart && bar.duration > 0
            x: parent.width * bar.loopStart / Math.max(bar.duration, 1)
            width: Math.max(Metrics.gap2, parent.width * (bar.loopEnd - bar.loopStart) / Math.max(bar.duration, 1))
            height: parent.height
            color: Theme.color.accent
            opacity: Metrics.notchSoft
        }
        Repeater {
            model: bar.duration > 0 ? [bar.loopStart, bar.loopEnd].filter(at => at >= 0) : []
            Rectangle {
                required property real modelData
                objectName: "loopMark"
                x: track.width * modelData / bar.duration - width / 2 // derived: centered on the mark
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.gap2
                height: Metrics.gap10
                color: Theme.color.accent
            }
        }
    }

    Rectangle {
        visible: bar.active && bar.duration > 0
        width: Metrics.seekKnob
        height: width
        radius: width / 2 // derived: a circle
        x: (bar.duration > 0 ? track.width * Math.min(1, bar.shown / bar.duration) : 0) - width / 2 // derived: centered on the fill's end
        anchors.verticalCenter: track.verticalCenter
        color: Theme.color.accent
    }

    Rectangle {
        visible: bar.active && bar.duration > 0 && bar.bubbleText !== ""
        width: label.implicitWidth + Metrics.gap10
        height: label.implicitHeight + Metrics.gap4
        radius: Metrics.radiusItem
        color: Theme.color.surfaceRaised
        x: Math.max(0, Math.min(bar.width - width, area.mouseX - width / 2)) // derived: over the pointer, inside the bar
        anchors.bottom: track.top
        anchors.bottomMargin: Metrics.gap6

        Edge { radius: parent.radius }

        Text {
            id: label
            anchors.centerIn: parent
            text: bar.bubbleText
            font.pointSize: Type.micro
            font.features: { "tnum": 1 }
            color: Theme.color.textPrimary
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        enabled: bar.duration > 0
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        preventStealing: true
        onPressed: (mouse) => bar.dragged = bar.secondsAt(mouse.x)
        onPositionChanged: (mouse) => { if (pressed) bar.dragged = bar.secondsAt(mouse.x) }
        onReleased: (mouse) => bar.seek(bar.dragged)
    }
}
