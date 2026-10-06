import QtQuick
import "../Theme"

// A grip to resize a panel by dragging (design/03 ResizeHandle): a notch in
// the middle, the resize cursor, and the distance dragged as it moves. The
// owner decides where the panel lands on release; nothing animates while
// the pointer moves.
//
// `moved` carries the distance from where the press started, not from the
// last move: the owner keeps its size at `started` and adds the distance to
// that. Adding it to the size already moved counts every step again.
Item {
    id: handle

    // Vertical handles resize across (a sidebar); horizontal ones, up and
    // down (a split).
    property bool vertical: true

    signal started()
    signal moved(real delta)
    signal released()

    implicitWidth: vertical ? Metrics.resizeHandle : Metrics.notchLong
    implicitHeight: vertical ? Metrics.notchLong : Metrics.panesHandle

    Rectangle {
        anchors.centerIn: parent
        width: handle.vertical ? Metrics.notchShort : Metrics.notchLong
        height: handle.vertical ? Metrics.notchLong : Metrics.notchShort
        radius: Metrics.gap2
        color: Theme.color.textTertiary
        opacity: area.containsMouse || area.pressed ? Metrics.notchStrong : Metrics.notchSoft
    }

    MouseArea {
        id: area

        property real start: 0

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: handle.vertical ? Qt.SizeHorCursor : Qt.SizeVerCursor
        // In window coordinates: the handle moves with what it resizes, and
        // its own coordinates would move with it.
        function along(mouse) {
            const p = mapToItem(null, mouse.x, mouse.y)
            return handle.vertical ? p.x : p.y
        }

        onPressed: (mouse) => {
            start = along(mouse)
            handle.started()
        }
        onPositionChanged: (mouse) => {
            if (pressed)
                handle.moved(along(mouse) - start)
        }
        onReleased: handle.released()
    }
}
