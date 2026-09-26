import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The head of an artist's or a playlist's page (design/06): the picture --
// a round portrait, or a mosaic of four covers -- what it is, its name, what
// it holds, and play, shuffle or queue all of it.
Item {
    id: hero

    property string label
    property string name
    property url art
    // Up to four covers; two or more make a mosaic.
    property var covers: []
    property bool round: false
    property string meta
    property bool canQueue: false

    signal play()
    signal shuffle()
    signal queue()

    readonly property bool mosaic: covers.length > 1

    implicitHeight: Metrics.pageHero

    ArtTile {
        id: picture
        visible: !hero.mosaic
        width: Metrics.pageHero
        height: Metrics.pageHero
        radius: hero.round ? width / 2 : Metrics.radiusCard // derived: a circle
        source: hero.covers.length === 1 ? hero.covers[0] : hero.art
    }

    Grid {
        visible: hero.mosaic
        width: Metrics.pageHero
        height: Metrics.pageHero
        columns: 2

        Repeater {
            model: 4
            ArtTile {
                required property int index
                width: Metrics.pageHero / 2 // derived: two to a side
                height: width
                radius: 0
                tint: index
                source: hero.covers[index] || ""
            }
        }
    }

    Column {
        anchors.left: picture.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        Text {
            text: hero.label.toUpperCase()
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
            IconButton {
                visible: hero.canQueue
                anchors.verticalCenter: parent.verticalCenter
                round: true
                glyph: Icons.playlistAdd
                tip: I18n.tr("Add to queue", I18n.rev)
                onClicked: hero.queue()
            }
        }
    }
}
