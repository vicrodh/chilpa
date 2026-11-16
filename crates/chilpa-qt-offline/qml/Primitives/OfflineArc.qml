import QtQuick
import QtQuick.Window
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The moving part of a track's offline mark, loaded only on the row whose
// file is on its way: an arc that fills to `fraction`, or a quarter that
// turns while the size is unknown (`fraction` below 0). Each half is the
// ring clipped to one side and turned, so nothing is painted again as it
// moves; the window's shared clock turns it only while it can be seen.
Item {
    id: arc
    required property QtObject clock
    property real fraction: -1
    property color color: Theme.color.accent
    readonly property bool sweeping: fraction < 0
    readonly property real angle: sweeping ? 90 : Math.max(0, Math.min(1, fraction)) * 360 // derived: a quarter while it turns; degrees of a turn
    readonly property bool windowPresented: !!Window.window && Window.window.visible && Window.window.visibility !== Window.Minimized
    readonly property bool moving: !!clock && sweeping && visible && windowPresented && !Motion.reduced
    property bool holding: false
    property real phase: 0
    rotation: sweeping ? phase * 360 : 0 // derived: one turn per loop

    function sync() {
        if (moving === holding) return
        holding = moving
        if (holding) clock.hold()
        else clock.release()
    }
    onMovingChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) clock.release()
    Connections {
        target: arc.moving ? arc.clock : null
        function onMsChanged() { arc.phase = (arc.phase + arc.clock.period / Motion.busyLoop) % 1 }
    }

    component Ring: Rectangle {
        width: arc.width
        height: arc.height
        radius: width / 2 // derived: a circle
        color: "transparent"
        border.width: Metrics.offlineRingStroke
        border.color: arc.color
    }

    // Where the arc runs.
    Rectangle {
        anchors.fill: parent
        radius: width / 2 // derived: a circle
        color: "transparent"
        border.width: Metrics.offlineRingStroke
        border.color: Theme.color.fillField
    }
    // From the top to the angle, up to half a turn: the ring's left half,
    // turned into the right half's window.
    Item {
        objectName: "offlineArcRight"
        x: arc.width / 2 // derived: the right half
        width: arc.width / 2 // derived: the right half
        height: arc.height
        clip: true
        Item {
            x: -arc.width / 2 // derived: back over the whole ring
            width: arc.width
            height: arc.height
            rotation: Math.min(arc.angle, 180) // derived: half a turn at most
            Item {
                width: arc.width / 2 // derived: the left half
                height: arc.height
                clip: true
                Ring {}
            }
        }
    }
    // Past half a turn: the ring's right half, turned into the left half's
    // window.
    Item {
        objectName: "offlineArcLeft"
        visible: arc.angle > 180 // derived: past half a turn
        width: arc.width / 2 // derived: the left half
        height: arc.height
        clip: true
        Item {
            width: arc.width
            height: arc.height
            rotation: arc.angle - 180 // derived: what passes half a turn
            Item {
                x: arc.width / 2 // derived: the right half
                width: arc.width / 2 // derived: the right half
                height: arc.height
                clip: true
                Ring { x: -arc.width / 2 } // derived: back over the whole ring
            }
        }
    }
}
