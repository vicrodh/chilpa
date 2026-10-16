import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The atmosphere drawn as Images: the arm the software scene graph takes,
// and the plain cover while the bitmap is not ready. The picture the one-pass
// shader has to match, layer for layer.
Item {
    id: layers

    property bool shown: true
    property url source
    property url fallbackSource
    property real dim: 0
    // Per layer: the drift in x and y (-1..1) and the opacity.
    property var drifts: [[0, 0], [0, 0], [0, 0], [0, 0]]
    property var opacities: [0.95, 0.48, 0.24, 0.18]

    readonly property bool ready: source.toString() !== ""

    Repeater {
        model: layers.shown && layers.ready ? Field.layers : []
        Image {
            required property int index
            required property var modelData
            source: layers.source
            retainWhileLoading: true
            fillMode: Image.PreserveAspectCrop
            width: layers.width + modelData.grow.width
            height: layers.height + modelData.grow.height
            x: modelData.origin.x + layers.drifts[index][0] * modelData.swing.x
            y: modelData.origin.y + layers.drifts[index][1] * modelData.swing.y
            opacity: layers.opacities[index]
        }
    }

    // The plain cover, oversized and quiet, until the bitmap exists.
    Image {
        visible: layers.shown && !layers.ready && layers.fallbackSource.toString() !== ""
        source: layers.fallbackSource
        fillMode: Image.PreserveAspectCrop
        width: layers.width + Field.staticGrow
        height: layers.height + Field.staticGrow
        x: -Field.staticGrow / 2 // derived: centered
        y: -Field.staticGrow / 2 // derived: centered
        opacity: Field.staticOpacity
    }

    // The dim and the scrim, as nodes: the shader bakes both in.
    Rectangle {
        anchors.fill: parent
        visible: layers.shown
        color: Theme.color.ambientVeil
        opacity: layers.dim
    }
    Rectangle {
        anchors.fill: parent
        visible: layers.shown
        gradient: Gradient {
            GradientStop { position: 0.0; color: Theme.color.albumHeaderTop }
            GradientStop { position: 0.35; color: "transparent" }
            GradientStop { position: 1.0; color: Theme.color.albumHeaderBottom }
        }
    }
}
