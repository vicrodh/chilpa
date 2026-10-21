import QtQuick
import QtQuick.Window
import "../Theme"

// I borrow the shared clock only while a pending action is visible.
Icon {
    id: mark
    property bool active: true
    required property QtObject clock
    readonly property bool windowPresented: !!Window.window && Window.window.visible && Window.window.visibility !== Window.Minimized
    readonly property bool moving: !!clock && active && visible && windowPresented && opacity > 0 && !Motion.reduced
    property bool holding: false
    property real phase: 0
    glyph: Icons.refresh
    rotation: phase * 360

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
        target: mark.moving ? mark.clock : null
        function onMsChanged() { mark.phase = (mark.phase + mark.clock.period / Motion.busyLoop) % 1 }
    }
}
