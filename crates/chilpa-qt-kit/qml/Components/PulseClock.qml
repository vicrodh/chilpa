import QtQuick
import app.chilpa.kit

QtObject {
    id: clock
    property bool running: false
    property bool holding: false
    signal triggered()
    function sync() {
        if (running === holding) return
        holding = running
        holding ? Pulse.hold() : Pulse.release()
    }
    onRunningChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) Pulse.release()
    property Connections tick: Connections {
        target: clock.holding ? Pulse : null
        function onMsChanged() { clock.triggered() }
    }
}
