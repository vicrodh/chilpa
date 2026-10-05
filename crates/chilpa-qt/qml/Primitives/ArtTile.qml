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

    // A file on disk, as a path: it becomes the source, each part of it
    // escaped, so spaces, brackets and a '#' in a folder name survive.
    property string path
    property url source: path === "" ? "" : "file://" + path.split("/").map(encodeURIComponent).join("/")
    property int radius: Metrics.radiusCard
    // Top left, top right, bottom right, bottom left: a tile of a mosaic
    // rounds only the corner that is the mosaic's, and draws no hairline.
    property vector4d corners: Qt.vector4d(radius, radius, radius, radius)
    property bool edged: true
    // Which placeholder color, so a grid of empty covers is not one block.
    property int tint: 0
    property bool contain: false

    // True once there is a picture to draw, so whoever uses the tile can
    // put something of its own over the placeholder.
    readonly property bool ready: image.status === Image.Ready
    readonly property bool local: root.source.toString().startsWith("file://")
    readonly property int decode: Math.ceil(Math.max(width, height) * Screen.devicePixelRatio / Metrics.artDecodeStep) * Metrics.artDecodeStep

    Rectangle {
        anchors.fill: parent
        visible: image.status !== Image.Ready
        topLeftRadius: root.corners.x
        topRightRadius: root.corners.y
        bottomRightRadius: root.corners.z
        bottomLeftRadius: root.corners.w
        color: Theme.placeholder(root.tint)
    }

    Image {
        id: image
        anchors.fill: parent
        visible: false
        // A file comes through the covers' provider, made the size it is
        // drawn; anything else as it is.
        source: root.local ? (root.contain ? "image://contained/" : "image://cover/") + root.decode + "/" + encodeURIComponent(root.source.toString()) : root.source
        asynchronous: true
        retainWhileLoading: true
        fillMode: Image.PreserveAspectCrop
        // The smallest decode is still several times a tile of a mosaic:
        // drawn that much smaller without its reductions, a cover is noise.
        mipmap: root.decode > Math.max(root.width, root.height) * Screen.devicePixelRatio * 2 // derived: less than half the decode
        sourceSize.width: root.decode
        sourceSize.height: root.decode
    }

    ShaderEffect {
        anchors.fill: parent
        visible: image.status === Image.Ready
        property variant source: image
        property size dims: Qt.size(width, height)
        property vector4d corners: root.corners
        fragmentShader: "../../assets/shaders/rounded.frag.qsb"
    }

    // Over the cover and over the placeholder alike, as the design's inset
    // hairline.
    Edge { visible: root.edged; radius: root.radius }
}
