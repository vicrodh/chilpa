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
    // An artist: a round portrait and the text centered under it
    // (design/03 ArtistCard).
    property bool round: false

    signal open()
    signal play()

    implicitHeight: width + Metrics.cardTextGap + Metrics.cardTextHeight

    ArtTile {
        id: cover
        width: card.width
        height: card.width
        radius: card.round ? width / 2 : Metrics.radiusCard // derived: a circle
        source: card.art
        tint: card.tint
    }

    Loader {
        anchors.fill: cover
        active: hover.hovered && !card.round
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
            horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: card.subtitle
            horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    HoverHandler { id: hover }
    TapHandler { onTapped: card.open() }
}
