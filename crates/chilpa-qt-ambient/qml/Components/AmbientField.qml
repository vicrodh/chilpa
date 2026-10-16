import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The album-colored field behind the whole window (mode 1), as QBZ's Qt
// shell draws it: a ShaderEffect over ambient.frag, inline and opaque,
// advanced on the shared pulse; a Canvas arm where the scene graph has no
// shaders (software, null) or the shader fails to load.
//
// It moves only while `running`, visible, and the window is neither
// minimized nor hidden: a tiling desktop keeps windows visible and unfocused
// all the time, so losing focus is never a reason to freeze. While it moves
// it holds the pulse; the moment it stops, it lets go.
Item {
    id: field

    property color primary
    property color secondary
    property color accent
    // The host freezes the clock while the field cannot be seen.
    property bool running: true
    // The legibility dim, multiplied into the shader toward black: 0 on
    // light themes, where the host's veil takes over.
    property real dim: 0

    readonly property bool windowShowing: field.Window.window
        ? (field.Window.window.visibility !== Window.Minimized && field.Window.window.visibility !== Window.Hidden)
        : true
    // The software and null scene graphs draw nothing for a ShaderEffect,
    // and a shader that fails to load must not blank the background either.
    readonly property bool noShaders: GraphicsInfo.api === GraphicsInfo.Software || GraphicsInfo.api === GraphicsInfo.Null
    readonly property bool shaderOk: !noShaders && effect.status !== ShaderEffect.Error
    readonly property bool moving: running && visible && windowShowing
    readonly property real pixelRatio: Screen.devicePixelRatio

    // The scene clock, in seconds; seeded so two launches differ.
    property real t: Math.random() * Field.seedSpan

    // One hold on the pulse while moving, let go when not: the same call
    // whether the change comes while the item is built or later.
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

    // The GPU arm needs nothing but the clock bump: `effect.time` binds `t`, so
    // this alone dirties the shader. A pulse while frozen writes nothing.
    Connections {
        target: Pulse
        function onMsChanged() {
            if (!field.moving)
                return
            field.t += Field.tickSeconds
            if (!field.shaderOk)
                canvas.requestPaint()
        }
    }

    ShaderEffect {
        id: effect
        anchors.fill: parent
        visible: field.shaderOk
        // Opaque output (alpha = qt_Opacity = 1): the bottom-most layer of
        // the window has nothing to blend against.
        blending: false

        // Matched BY NAME against the uniform block in ambient.frag.
        property color primary: field.primary
        property color secondary: field.secondary
        property color accent: field.accent
        property real time: field.t
        // The audio breathe is not wired: at 0 the geometry is the reference's.
        property real levelSmooth: 0.0
        // The framebuffer's size: the aspect and the grain describe the
        // pixels actually written.
        property real resX: Math.max(1, Math.round(field.width * field.pixelRatio))
        property real resY: Math.max(1, Math.round(field.height * field.pixelRatio))
        property real dim: field.dim

        onStatusChanged: if (status === ShaderEffect.Error) console.warn("AmbientField: shader failed:", log)

        fragmentShader: "../../assets/shaders/ambient.frag.qsb"
        // The paired vertex stage: the GL link wants both stages to declare
        // the same uniform block.
        vertexShader: "../../assets/shaders/ambient.vert.qsb"
    }

    AmbientCanvas {
        id: canvas
        anchors.fill: parent
        visible: !field.shaderOk
        primary: field.primary
        secondary: field.secondary
        accent: field.accent
        t: field.t
    }
}
