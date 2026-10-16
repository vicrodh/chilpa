import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The footer's seek bar (02-mini §1.5, QBZ's MiniSeek): a thin track
// inside a tall hit area, no thumb, no hover growth, no buffered part. It
// keeps no state: it says where the pointer is while it is down and where
// it let go, and the footer holds the position asked for until the player
// lands on it. Not clamped to what can be sought, as QBZ's is not.
Item {
    id: seek

    // The effective share played, 0 to 1, as the footer computes it.
    property real progress: 0
    property int trackHeight: Card.seekTrack[0]
    // Micro's bar is square-ended.
    property bool rounded: true
    property bool lightForeground: false

    signal preview(real fraction)
    signal commit(real fraction)

    function fractionAt(x) {
        return seek.width > 0 ? Math.max(0, Math.min(1, x / seek.width)) : 0
    }

    Rectangle {
        id: track
        width: seek.width
        height: seek.trackHeight
        y: Math.round((seek.height - height) / 2) // derived: centered in the hit area
        radius: seek.rounded ? height / 2 : 0 // derived: round ends
        color: seek.lightForeground ? Theme.color.miniSeekOnBackdrop : Theme.color.miniCapsuleHover
        clip: true

        Rectangle {
            width: parent.width * Math.max(0, Math.min(1, seek.progress))
            height: parent.height
            radius: parent.radius
            color: Theme.color.accent
        }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onPressed: (mouse) => seek.preview(seek.fractionAt(mouse.x))
        onPositionChanged: (mouse) => { if (pressed) seek.preview(seek.fractionAt(mouse.x)) }
        onReleased: (mouse) => seek.commit(seek.fractionAt(mouse.x))
    }
}
