// hot-path: at most four bounded cover derivatives per visible genre tile.
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: mosaic
    property var images: []
    // Shown on hover while the images are a published default.
    property string credit: ""
    property bool activeImages: true
    // Without images the mosaic is one flat tile of this placeholder color.
    property int tint: 0
    property int radius: Metrics.radiusCard
    onActiveImagesChanged: if (!activeImages) TipState.hide(mosaic)
    Component.onDestruction: TipState.hide(mosaic)
    readonly property int count: Math.min(4, images.length)
    ArtTile { objectName: "discoveryMosaicFlat"; anchors.fill: parent; radius: mosaic.radius; visible: mosaic.count === 0; tint: mosaic.tint }
    HoverHandler {
        onHoveredChanged: hovered && mosaic.count > 0 && mosaic.credit !== "" ? TipState.show(mosaic.credit, mosaic) : TipState.hide(mosaic)
    }
    Repeater {
        model: mosaic.count
        ArtTile {
            objectName: "discoveryMosaicImage" + index
            required property int index
            readonly property bool east: mosaic.count === 3 ? index > 0 : index % 2 === 1
            readonly property bool south: mosaic.count === 3 ? index === 2 : mosaic.count === 4 && index >= 2
            readonly property bool splitHeight: mosaic.count === 4 || mosaic.count === 3 && index > 0
            x: east ? mosaic.width / 2 : 0
            y: south ? mosaic.height / 2 : 0
            width: mosaic.count === 1 ? mosaic.width : mosaic.width / 2 // derived: two mosaic columns
            height: splitHeight ? mosaic.height / 2 : mosaic.height // derived: two mosaic rows
            source: mosaic.activeImages ? mosaic.images[index] : ""
            radius: mosaic.radius; edged: false
            corners: Qt.vector4d(!east && !south ? radius : 0, (east || mosaic.count === 1) && !south ? radius : 0,
                (east || mosaic.count === 1) && (south || !splitHeight) ? radius : 0, !east && (south || !splitHeight) ? radius : 0)
        }
    }
}
