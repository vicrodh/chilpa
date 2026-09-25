import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The head of a record's page: cover, kind and year, title, artist, the
// line of facts and the buttons (design/06 Album).
Item {
    id: hero

    property AlbumDetailModel detail

    signal play()
    signal shuffle()

    implicitHeight: Metrics.heroArt

    ArtTile {
        id: cover
        width: Metrics.heroArt
        height: Metrics.heroArt
        source: hero.detail.art
    }

    Column {
        anchors.left: cover.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        Text {
            text: hero.detail.kindLine
            font.pointSize: Type.micro
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            text: hero.detail.title
            elide: Text.ElideRight
            font.pointSize: Type.display
            font.weight: Type.bold
            font.letterSpacing: Type.displayTracking
            color: Theme.color.textPrimary
        }
        Text {
            text: hero.detail.artist
            font.pointSize: Type.body
            font.weight: Type.medium
            color: Theme.color.accent
        }
        Text {
            width: parent.width
            text: hero.detail.meta
            elide: Text.ElideRight
            font.pointSize: Type.row
            color: Theme.color.textSecondary
        }

        Item { width: 1; height: Metrics.gap8 }

        Row {
            spacing: Metrics.gap8
            PillButton {
                kind: "primary"
                glyph: Icons.playerPlayFilled
                filled: true
                text: I18n.tr("Play", I18n.rev)
                onClicked: hero.play()
            }
            PillButton {
                glyph: Icons.arrowsShuffle
                text: I18n.tr("Shuffle", I18n.rev)
                onClicked: hero.shuffle()
            }
        }
    }
}
