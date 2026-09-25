import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The head of an artist's page: a round portrait, "ARTIST", the name, what
// the library has of them, and play or shuffle all of it.
Item {
    id: hero

    property string name
    property url art
    property string meta

    signal play()
    signal shuffle()

    implicitHeight: Metrics.artistHero

    ArtTile {
        id: portrait
        width: Metrics.artistHero
        height: Metrics.artistHero
        radius: width / 2 // derived: a circle
        source: hero.art
    }

    Column {
        anchors.left: portrait.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        Text {
            text: I18n.tr("Artist", I18n.rev).toUpperCase()
            font.pointSize: Type.micro
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            text: hero.name
            elide: Text.ElideRight
            font.pointSize: Type.display
            font.weight: Type.bold
            font.letterSpacing: Type.displayTracking
            color: Theme.color.textPrimary
        }
        Text {
            text: hero.meta
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
