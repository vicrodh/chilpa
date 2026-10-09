import QtQuick
import app.chilpa.kit
import "../Theme"

// The "now playing" sign: four bars of 3 px in an 18 by 14 box, each a
// cosine of its own phase over an 800 ms loop advanced on the shared
// pulse, which it holds only while active and visible.
Item {
    id: bars
    property color tint: Theme.color.accent
    property real barScale: 1
    property bool active: true
    property real t: 0
    width: Metrics.eqWide * barScale
    height: Metrics.eqTall * barScale
    implicitWidth: width
    implicitHeight: height

    readonly property bool moving: active && visible
    property bool holding: false
    function sync() {
        if (moving === holding)
            return
        holding = moving
        if (holding)
            Pulse.hold()
        else
            Pulse.release()
    }
    onMovingChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) Pulse.release()

    Connections {
        target: bars.moving ? Pulse : null
        function onMsChanged() {
            bars.t = (bars.t + Pulse.period / Metrics.eqLoop) % 1
        }
    }
    function wave(phase) {
        const u = (bars.t + phase) % 1
        return 0.3 + 0.7 * (0.5 - 0.5 * Math.cos(u * 6.283185))
    }
    Repeater {
        model: [{ at: 0, base: 0.6, phase: 0 }, { at: 1, base: 1, phase: 0.25 }, { at: 2, base: 0.4, phase: 0.125 }, { at: 3, base: 0.8, phase: 0.375 }]
        Rectangle {
            required property var modelData
            x: modelData.at * Metrics.eqStep * bars.barScale
            width: Metrics.eqBar * bars.barScale
            y: bars.height - height
            height: bars.height * modelData.base * bars.wave(modelData.phase)
            radius: bars.barScale
            color: bars.tint
        }
    }
}
