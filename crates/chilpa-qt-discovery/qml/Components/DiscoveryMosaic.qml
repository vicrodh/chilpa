// hot-path: at most four bounded cover derivatives per visible genre tile.
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: mosaic
    property var images: []
    property var art: ({})
    property bool activeImages: true
    property string label: ""
    property int radius: Metrics.radiusCard
    onActiveImagesChanged: if (!activeImages) TipState.hide(mosaic)
    Component.onDestruction: TipState.hide(mosaic)
    readonly property int count: Math.min(4, images.length)
    ArtTile {
        anchors.fill: parent; radius: mosaic.radius; visible: mosaic.count === 0
        source: mosaic.activeImages && mosaic.art?.key ? "qrc:/qt/qml/app/chilpa/discovery/assets/genres/" + mosaic.art.key + ".jpg" : ""
    }
    HoverHandler {
        onHoveredChanged: hovered && mosaic.count === 0 && mosaic.art?.page
            ? TipState.show([mosaic.art.credit,mosaic.art.license,mosaic.art.licenseUrl,mosaic.art.page].filter(Boolean).join("\n"), mosaic) : TipState.hide(mosaic)
    }
    Text {
        anchors.centerIn: parent
        visible: mosaic.count === 0 && !mosaic.art?.key
        text: mosaic.label.split(/\s+/).filter(Boolean).slice(0, 2).map(word => word[0]).join("").toUpperCase()
        font.pointSize: Type.title; font.weight: Type.semibold; color: Theme.color.textTertiary
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
