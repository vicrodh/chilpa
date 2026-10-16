import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The mini player's cover tile (02-mini §1.5, QBZ's MiniCoverArt): a
// rounded, hairlined box whose tinted fill is the empty state; no glyph,
// no skeleton. The cover itself is the kit's tile, decoded once at the
// size shown.
Rectangle {
    id: cover

    property url source

    radius: Metrics.radiusControl
    antialiasing: true
    color: Theme.color.miniCoverFill
    border.width: Card.edge
    border.color: Theme.color.miniCapsuleHover
    clip: true

    ArtTile {
        anchors.fill: parent
        visible: cover.source.toString() !== ""
        source: cover.source
        radius: cover.radius
        edged: false
    }
}
