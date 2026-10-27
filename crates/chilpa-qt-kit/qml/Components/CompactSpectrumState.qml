// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import app.chilpa.kit

// Both surfaces share the same easing, bounded buffers and expiring peaks.
// A source binding observes only the selected stream; no capture is acquired.
QtObject {
    id: state
    property bool active: false
    property int mode: 0
    property var source: []
    property var targetValues: []
    property var values: []
    property var peaks: []
    property var holds: []
    property bool settling: false
    readonly property var buffers: ({ a: [], b: [], peaksA: [], peaksB: [], flip: false })
    function receive() {
        if (!state.active) return
        const source = state.source || []
        if (state.mode !== 1 && Array.isArray(source)) {
            targetValues = source
            settling = true
            return
        }
        const size = state.mode === 1 ? 48 : state.mode === 2 ? 5 : 16
        const next = new Array(size)
        for (let i = 0; i < size; ++i)
            next[i] = Math.max(0, Math.min(1, state.mode === 1 ? Math.abs(source[i * 5] || 0) : source[i] || 0))
        targetValues = next
        settling = true
    }
    function tick() {
        const next = buffers.flip ? buffers.a : buffers.b
        next.length = targetValues.length
        const withPeaks = state.mode === 5
        const tops = withPeaks ? (buffers.flip ? buffers.peaksA : buffers.peaksB) : null
        if (withPeaks) tops.length = targetValues.length
        let moved = false
        let peakChanged = false
        for (let i = 0; i < targetValues.length; ++i) {
            const old = values[i] || 0
            const delta = targetValues[i] - old
            const value = Math.abs(delta) > 0.0005 ? old + delta * 0.45 : targetValues[i]
            next[i] = value
            moved = moved || Math.abs(delta) > 0.0005
            if (!withPeaks) continue
            const previousPeak = peaks[i] || 0
            let peak = previousPeak
            let hold = Math.max(0, (holds[i] || 0) - Pulse.period)
            if (value > peak) { peak = value; hold = 160 }
            else if (hold === 0) peak = Math.max(value, peak - Pulse.period / 650)
            tops[i] = peak
            peakChanged = peakChanged || peak !== previousPeak
            holds[i] = hold
            if (state.mode === 5 && peak > value + 0.0005) moved = true
        }
        settling = moved
        if (!moved && !peakChanged) return
        buffers.flip = !buffers.flip
        values = next
        if (withPeaks) peaks = tops
    }
    function reset() { values = []; peaks = []; holds = []; settling = false }
    onSourceChanged: receive()
    onModeChanged: { reset(); receive() }
    onActiveChanged: { if (active) receive(); else reset() }
    Component.onCompleted: receive()
    property PulseClock clock: PulseClock {
        running: state.active && state.settling
        onTriggered: state.tick()
    }
}
