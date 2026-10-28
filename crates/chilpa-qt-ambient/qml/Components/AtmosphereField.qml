import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The blurred-cover background (modes 2 and 4), as QBZ's Qt shell draws it:
// four layers of the same 128 px atmosphere bitmap, each oversized past the
// viewport and drifting on its own sine pair, under a dim and a scrim. One
// ShaderEffect composites all of it in a single opaque pass; where shaders
// are not available the four Images draw the same picture (AtmosphereLayers).
//
// The drift accumulates `tick` on the shared pulse while `animated`, and
// resets to 0 when the drift stops, so the still pose is always the same
// one. While it drifts it holds the pulse.
Item {
    id: field

    // The atmosphere bitmap; empty while it is not ready. Whoever publishes
    // it never clears it on a track change, so the picture never flashes.
    property url source
    // The plain cover, shown instead while the bitmap is missing.
    property url fallbackSource
    property real dim: 0
    property real baseOpacity: 0.95
    property real warpOpacity: 0.48
    property bool animated: true

    property real tick: 0
    readonly property bool noShaders: GraphicsInfo.api === GraphicsInfo.Software || GraphicsInfo.api === GraphicsInfo.Null
    readonly property bool shaderOk: !noShaders && effect.status !== ShaderEffect.Error
    readonly property bool ready: source.toString() !== ""
    readonly property bool moving: animated && visible

    clip: true

    // One hold on the pulse while drifting, let go when still.
    property bool holding: false
    function sync() {
        if (moving === holding)
            return
        holding = moving
        holding ? Pulse.hold() : Pulse.release()
    }
    onMovingChanged: sync()
    onAnimatedChanged: if (!animated) { tick = 0; advance() }
    onWidthChanged: advance()
    onHeightChanged: advance()
    Component.onCompleted: { sync(); advance() }
    Component.onDestruction: if (holding) Pulse.release()

    Connections {
        target: Pulse
        function onMsChanged() {
            if (field.moving) {
                field.tick += Field.atmosphereTick
                field.advance()
            }
        }
    }

    // The eight sine pairs and the four layer vectors, computed in ONE pass
    // per pulse and written into the effect. As a chain of readonly
    // bindings and var arrays re-evaluated on every tick they cost 1.6
    // points of a core on the window's thread; as one function, 0.4
    // (measured 2026-10-27, Queues with the blurred cover).
    readonly property real tau: 6.283185
    // Each layer's drift, in the sign the layers use; the software arm reads it.
    property var drifts: [[0, 0], [0, 0], [0, 0], [0, 0]]
    readonly property var opacities: [baseOpacity, warpOpacity, Field.layers[2].opacity, Field.layers[3].opacity]
    function driftsAt(t) {
        const s1x = Math.sin(t / 7600.0 * tau) + 0.35 * Math.sin((t / 3300.0 + 1.9) * tau)
        const s1y = Math.sin((t / 11200.0 + 1.1) * tau)
        const s2x = Math.sin((t / 9200.0 + 0.4) * tau) + 0.42 * Math.sin((t / 4100.0 + 2.6) * tau)
        const s2y = Math.sin((t / 14800.0 + 1.7) * tau)
        const s3x = Math.sin((t / 5700.0 + 1.6) * tau)
        const s3y = Math.sin((t / 12300.0 + 0.2) * tau) + 0.28 * Math.sin((t / 3600.0 + 2.8) * tau)
        const s4x = Math.sin((t / 15100.0 + 2.4) * tau)
        const s4y = Math.sin((t / 6900.0 + 1.4) * tau)
        return [[s1x, s1y], [-s2x, -s2y], [s3x, s3y], [-s4x, s4y]]
    }

    // One layer the way PreserveAspectCrop of a SQUARE source composes: the
    // cover scale is the larger side of the grown layer, the crop is
    // centred, so a fragment at item pixel p samples (p - centre) / scale +
    // 0.5. Returns (centre x, centre y, scale, opacity).
    function packLayer(i, drift) {
        const layer = Field.layers[i]
        const grownWidth = field.width + layer.grow.width
        const grownHeight = field.height + layer.grow.height
        const originX = layer.origin.x + drift[0] * layer.swing.x
        const originY = layer.origin.y + drift[1] * layer.swing.y
        return Qt.vector4d(originX + grownWidth / 2, originY + grownHeight / 2, Math.max(grownWidth, grownHeight), field.opacities[i])
    }
    function advance() {
        // A size change before the children exist has nothing to write to.
        if (!effect)
            return
        const d = driftsAt(tick)
        effect.l1 = packLayer(0, d[0])
        effect.l2 = packLayer(1, d[1])
        effect.l3 = packLayer(2, d[2])
        effect.l4 = packLayer(3, d[3])
        if (!shaderOk)
            drifts = d
    }

    // The root surface stays under the shader too: an opaque node under an
    // opaque effect is discarded early, and the plain-cover path needs it.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.albumHeaderBase
    }

    // The bitmap as a plain sampler, never shown itself.
    Image {
        id: bitmap
        visible: false
        source: field.visible ? field.source : ""
        sourceSize: Qt.size(128, 128)
        retainWhileLoading: true
        smooth: true
    }

    ShaderEffect {
        id: effect
        anchors.fill: parent
        visible: field.shaderOk && field.ready
        blending: false

        property var tex: bitmap
        property real resX: width
        property real resY: height
        property real dim: field.dim
        // Written by advance(), never bound: see the note above it.
        property vector4d l1
        property vector4d l2
        property vector4d l3
        property vector4d l4

        onStatusChanged: if (status === ShaderEffect.Error) console.warn("AtmosphereField: shader failed:", log)

        fragmentShader: "../../assets/shaders/atmosphere.frag.qsb"
        vertexShader: "../../assets/shaders/atmosphere.vert.qsb"
    }

    AtmosphereLayers {
        anchors.fill: parent
        shown: field.visible && (!field.shaderOk || !field.ready)
        source: field.source
        fallbackSource: field.fallbackSource
        dim: field.dim
        drifts: field.drifts
        opacities: field.opacities
    }
}
