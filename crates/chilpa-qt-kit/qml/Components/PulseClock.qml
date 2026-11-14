import QtQuick
import app.chilpa.kit

QtObject {
    id: clock
    property bool running: false
    property bool holding: false
    property bool slow: false
    property bool holdingSlow: false
    property int minimumInterval: 0
    property int lastEdge: -1
    signal triggered()
    function sync() {
        if (running === holding && (!holding || slow === holdingSlow)) return
        if (holding) holdingSlow ? Pulse.releaseSlow() : Pulse.release()
        holding = running
        holdingSlow = slow
        lastEdge = -1
        if (holding) holdingSlow ? Pulse.holdSlow() : Pulse.hold()
    }
    onRunningChanged: sync()
    onSlowChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) holdingSlow ? Pulse.releaseSlow() : Pulse.release()
    property Connections tick: Connections {
        target: clock.holding ? Pulse : null
        function onMsChanged() {
            if (clock.lastEdge < 0 || Pulse.ms < clock.lastEdge || Pulse.ms - clock.lastEdge >= clock.minimumInterval) {
                clock.lastEdge = Pulse.ms
                clock.triggered()
            }
        }
    }
}
