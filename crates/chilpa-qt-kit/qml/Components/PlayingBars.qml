import QtQuick
import app.chilpa.kit
import "../Theme"

// The "now playing" sign, QBZ's EqualizerBars: four bars of 3 px in an 18
// by 14 box at x 0, 5, 10 and 15, each bottom-aligned by its own animated
// height, base heights 60, 100,
// 40 and 80 % sweeping 0.3 to 1.0 on an 800 ms loop with phases 0, 0.25,
// 0.125 and 0.375. The loop advances on the shared pulse, which the bars
// hold only while active and visible: an inactive one disconnects instead
// of waking a handler just to return. A fixed shader quad changes only its
// phase uniform; software rendering and shader errors use the same rectangles.
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

    property bool shaderFailed: false
    function supportsShaders(api) {
        return api === GraphicsInfo.OpenGL || api === GraphicsInfo.Vulkan
            || api === GraphicsInfo.Metal || api === GraphicsInfo.Direct3D11
            || api === GraphicsInfo.Direct3D12
    }
    Loader {
        objectName: "playingBarsRenderer"
        anchors.fill: parent
        sourceComponent: bars.supportsShaders(GraphicsInfo.api) && !bars.shaderFailed ? shaded : rectangles
    }
    Component {
        id: shaded
        Item {
            objectName: "playingBarsShaderRenderer"
            ShaderEffect {
                objectName: "playingBarsShader"
                x: -bars.barScale; y: -bars.barScale
                width: bars.width + 2 * bars.barScale // derived: coverage beyond both horizontal edges
                height: bars.height + 2 * bars.barScale // derived: coverage beyond both vertical edges
                property size dims: Qt.size(bars.width, bars.height)
                property color tint: bars.tint
                property vector2d geometry: Qt.vector2d(Metrics.eqBar * bars.barScale, Metrics.eqStep * bars.barScale)
                property real radius: bars.barScale
                property real phase: bars.t
                fragmentShader: "../../assets/shaders/playing-bars.frag.qsb"
                onStatusChanged: if (status === ShaderEffect.Error) bars.shaderFailed = true
            }
        }
    }
    Component {
        id: rectangles
        Item {
            objectName: "playingBarsRectangles"
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
