// hot-path: drawn once per album card and per row that shows a cover.
import QtQuick
import "../Theme"

// A cover with rounded corners and a hairline edge, or the placeholder
// color while it loads (design/03 ArtTile). A cover on disk is never decoded
// whole: it is asked of the covers' provider at the size drawn, which Rust
// makes once and keeps (frontend ADR-0014, rule 5).
//
// The corners come from one shader pass over the image's own texture: no
// layer, no mask, no offscreen render per cover. The hairline is an outline
// on top, which batches with every other outline on screen. The decode size is rounded
// up to a step of 128 device pixels, so resizing the window does not decode
// every cover again at every pixel of the drag, and two tiles of nearly the
// same size share one decode.
Item {
    id: root

    property url source
    property int radius: Metrics.radiusCard
    // Which placeholder color, so a grid of empty covers is not one block.
    property int tint: 0

    readonly property bool local: root.source.toString().startsWith("file://")
    readonly property int decode: Math.ceil(Math.max(width, height) * Screen.devicePixelRatio / Metrics.artDecodeStep) * Metrics.artDecodeStep

    Rectangle {
        anchors.fill: parent
        visible: image.status !== Image.Ready
        radius: root.radius
        color: Theme.placeholder(root.tint)
    }

    Image {
        id: image
        anchors.fill: parent
        visible: false
        // A file comes through the covers' provider, made the size it is
        // drawn; anything else as it is.
        source: root.local ? "image://cover/" + root.decode + "/" + encodeURIComponent(root.source.toString()) : root.source
        asynchronous: true
        retainWhileLoading: true
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: root.decode
        sourceSize.height: root.decode
    }

    ShaderEffect {
        anchors.fill: parent
        visible: image.status === Image.Ready
        property variant source: image
        property size dims: Qt.size(width, height)
        property real radius: root.radius
        fragmentShader: "../../assets/shaders/rounded.frag.qsb"
    }

    // Over the cover and over the placeholder alike, as the design's inset
    // hairline.
    Edge { radius: root.radius }
}
