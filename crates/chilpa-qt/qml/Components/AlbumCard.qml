// hot-path: one per album in a grid, recycled with its row.
import QtQuick
import "../Theme"
import "../Primitives"

// A record: its cover, its title and a subtitle (design/03 AlbumCard). The
// hover overlay is built only for the card under the pointer.
Item {
    id: card

    property string title
    property string subtitle
    property url art
    property int tint: 0

    signal open()
    signal play()

    implicitHeight: width + Metrics.cardTextGap + Metrics.cardTextHeight

    ArtTile {
        id: cover
        width: card.width
        height: card.width
        source: card.art
        tint: card.tint
    }

    Loader {
        anchors.fill: cover
        active: hover.hovered
        sourceComponent: AlbumCardOverlay {
            onPlay: card.play()
        }
    }

    Column {
        anchors.top: cover.bottom
        anchors.topMargin: Metrics.cardTextGap
        width: card.width

        Text {
            width: parent.width
            text: card.title
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: card.subtitle
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    HoverHandler { id: hover }
    TapHandler { onTapped: card.open() }
}
