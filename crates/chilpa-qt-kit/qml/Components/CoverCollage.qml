// hot-path: drawn once per playlist row.
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A playlist's covers in the space of an icon: with fewer than four, the
// first one whole; with four, a mosaic of two by two with a seam between
// them. Only the mosaic's outer corners are round, each tile rounding its
// own in the cover's one shader pass.
Item {
    id: collage

    // Paths of covers on disk, up to four.
    property var covers: []
    property int tint: 0

    readonly property bool mosaic: covers.length >= 4
    readonly property real tile: (width - Metrics.playlistCollageSeam) / 2 // derived: two tiles and the seam

    implicitWidth: Metrics.playlistCollage
    implicitHeight: Metrics.playlistCollage

    Repeater {
        model: collage.mosaic ? 4 : Math.min(1, collage.covers.length)

        ArtTile {
            required property int index
            readonly property bool east: index % 2 === 1
            readonly property bool south: index >= 2
            readonly property int round: Metrics.radiusRow

            x: east ? collage.tile + Metrics.playlistCollageSeam : 0
            y: south ? collage.tile + Metrics.playlistCollageSeam : 0
            width: collage.mosaic ? collage.tile : collage.width
            height: width
            radius: round
            corners: !collage.mosaic ? Qt.vector4d(round, round, round, round)
                : Qt.vector4d(!east && !south ? round : 0, east && !south ? round : 0,
                              east && south ? round : 0, !east && south ? round : 0)
            edged: !collage.mosaic
            path: collage.covers[index]
            tint: collage.tint + index
        }
    }
}
