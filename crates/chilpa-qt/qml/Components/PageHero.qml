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
    property bool canPlay: true
    // Off for what has no order to shuffle: a station, one episode.
    property bool canShuffle: true
    property string queueGlyph: Icons.playlistAdd
    property string queueIconFamily
    property string queueTip: I18n.tr("Add to queue", I18n.rev)
    property bool canSelect: false
    property bool selecting: false
    // What the listener's marks go on: a playlist takes stars and a heart,
    // an artist a heart.
    property string markKind
    property string markKey

    signal play()
    signal shuffle()
    signal queue(real left, real top)
    signal selectToggle()

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
                visible: hero.canPlay
                kind: "primary"
                glyph: Icons.playerPlayFilled
                filled: true
                text: I18n.tr("Play", I18n.rev)
                onClicked: hero.play()
            }
            PillButton {
                visible: hero.canPlay && hero.canShuffle
                glyph: Icons.arrowsShuffle
                text: I18n.tr("Shuffle", I18n.rev)
                onClicked: hero.shuffle()
            }
            MarkButtons {
                anchors.verticalCenter: parent.verticalCenter
                visible: hero.markKind !== ""
                kind: hero.markKind
                key: hero.markKey
                rates: hero.markKind !== "artist"
            }
            IconButton {
                id: queueButton
                visible: hero.canQueue
                anchors.verticalCenter: parent.verticalCenter
                round: true
                glyph: hero.queueGlyph
                iconFamily: hero.queueIconFamily
                tip: hero.queueTip
                onClicked: {
                    const at = queueButton.mapToItem(hero, 0,
                                                     queueButton.height + Metrics.gap4)
                    hero.queue(at.x, at.y)
                }
            }
            IconButton {
                visible: hero.canSelect
                anchors.verticalCenter: parent.verticalCenter
                round: true
                tone: hero.selecting ? "accent" : "normal"
                glyph: Icons.checkbox
                tip: hero.selecting ? I18n.tr("Finish selecting", I18n.rev)
                                    : I18n.tr("Select tracks", I18n.rev)
                onClicked: hero.selectToggle()
            }
        }
    }
}
