import QtQuick
import app.chilpa.kit
import "../Theme"

// The "now playing" sign, QBZ's EqualizerBars (`immersive/EqualizerBars.qml`)
// line for line: four bars of 3 px in an 18 by 14 box at x 0, 5, 10 and 15,
// each bottom-aligned by its own animated height, base heights 60, 100, 40
// and 80 % sweeping 0.3 to 1.0 on an 800 ms loop with phases 0, 0.25, 0.125
// and 0.375. The loop advances on the shared pulse, ~24 samples a loop, in
// the same frame as everything else the pulse moves; an inactive or hidden
// one lets the pulse go instead of waking a handler just to return.
//
// Where the scene graph has shaders the four bars are ONE fixed quad whose
// only change per pulse is the `phase` uniform (ADR-018, the indicator's
// quad). Four Rectangles whose heights change are four blended geometry
// nodes changing geometry on every pulse, and the renderer then rebuilds
// every alpha batch whose render order overlaps theirs
// (qsgbatchrenderer.cpp, nodeChanged: a geometry change in a batch that
// is not opaque invalidates the overlapping render orders): in a long list
// that is the whole list, re-merged and re-uploaded thirty times a second.
// The Rectangles stay as the arm for software and null scene graphs, where
// a ShaderEffect draws nothing, and for a shader that fails to load.
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

    readonly property bool windowShowing: !Window.window
        || (Window.window.visibility !== Window.Hidden && Window.window.visibility !== Window.Minimized)
    readonly property bool moving: active && visible && windowShowing
    readonly property bool noShaders: GraphicsInfo.api === GraphicsInfo.Software || GraphicsInfo.api === GraphicsInfo.Null
    property bool shaderFailed: false
    readonly property bool shaderOk: !noShaders && !shaderFailed
    property bool holding: false
    function sync() {
        if (moving === holding)
            return
        holding = moving
        holding ? Pulse.hold() : Pulse.release()
    }
    onMovingChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) Pulse.release()

    Connections {
        target: bars.moving ? Pulse : null
        function onMsChanged() { bars.t = (bars.t + Pulse.period / Metrics.eqLoop) % 1 }
    }
    function wave(phase) {
        const u = (bars.t + phase) % 1
        return 0.3 + 0.7 * (0.5 - 0.5 * Math.cos(u * 6.283185))
    }

    // The quad: the bars' box plus one bar-scale of coverage past every
    // edge, as the Rectangles' antialiasing has.
    ShaderEffect {
        id: quad
        objectName: "playingBarsShader"
        visible: bars.shaderOk
        x: -bars.barScale; y: -bars.barScale
        width: bars.width + 2 * bars.barScale // derived: coverage beyond both horizontal edges
        height: bars.height + 2 * bars.barScale // derived: coverage beyond both vertical edges
        property size dims: Qt.size(bars.width, bars.height)
        property color tint: bars.tint
        property vector2d geometry: Qt.vector2d(Metrics.eqBar * bars.barScale, Metrics.eqStep * bars.barScale)
        property real radius: bars.barScale
        property real phase: bars.t
        fragmentShader: "../../assets/shaders/playing-bars.frag.qsb"
        onStatusChanged: if (status === ShaderEffect.Error) { bars.shaderFailed = true; console.warn("PlayingBars: shader failed:", log) }
    }
    Loader {
        id: rectangles
        objectName: "playingBarsRectangles"
        active: !bars.shaderOk
        anchors.fill: parent
        sourceComponent: Item {
            Rectangle {
                x: 0
                width: Metrics.eqBar * bars.barScale
                y: bars.height - height
                height: bars.height * 0.6 * bars.wave(0)
                radius: bars.barScale
                color: bars.tint
            }
            Rectangle {
                x: Metrics.eqStep * bars.barScale
                width: Metrics.eqBar * bars.barScale
                y: bars.height - height
                height: bars.height * bars.wave(0.25)
                radius: bars.barScale
                color: bars.tint
            }
            Rectangle {
                x: Metrics.eqStep * 2 * bars.barScale // derived: the third bar
                width: Metrics.eqBar * bars.barScale
                y: bars.height - height
                height: bars.height * 0.4 * bars.wave(0.125)
                radius: bars.barScale
                color: bars.tint
            }
            Rectangle {
                x: Metrics.eqStep * 3 * bars.barScale // derived: the fourth bar
                width: Metrics.eqBar * bars.barScale
                y: bars.height - height
                height: bars.height * 0.8 * bars.wave(0.375)
                radius: bars.barScale
                color: bars.tint
            }
        }
    }
}
