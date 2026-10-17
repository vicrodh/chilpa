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
    onAnimatedChanged: if (!animated) tick = 0
    Component.onCompleted: sync()
    Component.onDestruction: if (holding) Pulse.release()

    Connections {
        target: Pulse
        function onMsChanged() {
            if (field.moving)
                field.tick += Field.atmosphereTick
        }
    }

    // The eight sine pairs, one source of truth for both arms.
    readonly property real tau: 6.283185
    readonly property real s1x: Math.sin(tick / 7600.0 * tau) + 0.35 * Math.sin((tick / 3300.0 + 1.9) * tau)
    readonly property real s1y: Math.sin((tick / 11200.0 + 1.1) * tau)
    readonly property real s2x: Math.sin((tick / 9200.0 + 0.4) * tau) + 0.42 * Math.sin((tick / 4100.0 + 2.6) * tau)
    readonly property real s2y: Math.sin((tick / 14800.0 + 1.7) * tau)
    readonly property real s3x: Math.sin((tick / 5700.0 + 1.6) * tau)
    readonly property real s3y: Math.sin((tick / 12300.0 + 0.2) * tau) + 0.28 * Math.sin((tick / 3600.0 + 2.8) * tau)
    readonly property real s4x: Math.sin((tick / 15100.0 + 2.4) * tau)
    readonly property real s4y: Math.sin((tick / 6900.0 + 1.4) * tau)
    // Each layer's drift, in the sign the layers use.
    readonly property var drifts: [[s1x, s1y], [-s2x, -s2y], [s3x, s3y], [-s4x, s4y]]
    readonly property var opacities: [baseOpacity, warpOpacity, Field.layers[2].opacity, Field.layers[3].opacity]

    // One layer the way PreserveAspectCrop of a SQUARE source composes: the
    // cover scale is the larger side of the grown layer, the crop is
    // centred, so a fragment at item pixel p samples (p - centre) / scale +
    // 0.5. Returns (centre x, centre y, scale, opacity).
    function packLayer(i) {
        const layer = Field.layers[i]
        const grownWidth = field.width + layer.grow.width
        const grownHeight = field.height + layer.grow.height
        const originX = layer.origin.x + field.drifts[i][0] * layer.swing.x
        const originY = layer.origin.y + field.drifts[i][1] * layer.swing.y
        return Qt.vector4d(originX + grownWidth / 2, originY + grownHeight / 2, Math.max(grownWidth, grownHeight), field.opacities[i])
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
        property vector4d l1: field.packLayer(0)
        property vector4d l2: field.packLayer(1)
        property vector4d l3: field.packLayer(2)
        property vector4d l4: field.packLayer(3)

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
