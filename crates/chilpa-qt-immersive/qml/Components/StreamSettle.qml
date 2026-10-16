import QtQuick
import app.chilpa.kit
import "../Theme"

// One applier for a whole visualizer stream (03-immersive §1.8). A
// publish stashes the frame and writes nothing; on the shared pulse, and
// only while live, the applied array eases toward the newest frame with
// `easeK` (0.45: about 83 % of the way in three pulses, QBZ's 90 ms), so
// every consumer of the pulse dirties the scene in the same turn and the
// window presents once. Converged, it writes nothing: a held signal
// repaints nothing. It holds the pulse only while live.
Item {
    id: settle
    // The newest frame, 0 to 1, as the stream publishes it or as the
    // binding that feeds this maps it.
    property var target: []
    // False freezes it: the publish path writes nothing and the last
    // applied frame holds.
    property bool live: true
    property real easeK: Stage.settleEase
    property var cur: []
    property var goal: null
    // Two buffers: a new reference every pulse notifies, and nothing is
    // allocated after the first two.
    readonly property var buffers: ({ a: [], b: [], flip: false })
    property var pending: null
    property bool holding: false

    visible: false
    width: 0
    height: 0

    function at(i) {
        const v = settle.cur[i] || 0
        return v < 0 ? 0 : (v > 1 ? 1 : v)
    }

    function sync() {
        if (live === holding)
            return
        holding = live
        if (holding)
            Pulse.hold()
        else
            Pulse.release()
    }
    onLiveChanged: {
        if (!live) {
            pending = null
            goal = null
        }
        sync()
    }
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) Pulse.release()

    onTargetChanged: {
        if (!settle.live)
            return
        const t = settle.target
        let arr
        if (Array.isArray(t)) {
            arr = t
        } else {
            const n = t ? t.length : 0
            arr = new Array(n)
            for (let i = 0; i < n; ++i)
                arr[i] = t[i]
        }
        settle.pending = arr
    }

    Connections {
        target: Pulse
        function onMsChanged() {
            if (!settle.live)
                return
            if (settle.pending !== null) {
                settle.goal = settle.pending
                settle.pending = null
            }
            const t = settle.goal
            if (t === null)
                return
            const n = t.length
            if (n === 0)
                return
            const source = settle.cur
            const dst = settle.buffers.flip ? settle.buffers.a : settle.buffers.b
            if (dst.length !== n)
                dst.length = n
            const k = settle.easeK
            let moved = false
            for (let i = 0; i < n; ++i) {
                const a = source[i] || 0
                const d = t[i] - a
                if (d > 0.0005 || d < -0.0005)
                    moved = true
                dst[i] = a + d * k
            }
            if (!moved)
                return
            settle.buffers.flip = !settle.buffers.flip
            settle.cur = dst
        }
    }
}
