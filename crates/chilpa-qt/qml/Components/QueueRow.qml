// hot-path: one per queued track, recycled while the queue scrolls.
import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A track in the side queue (design/03 QueueRow): cover, title and artist,
// time. The one that plays is marked; a double click plays from here, and
// the x on hover takes a track out.
Item {
    id: row

    required property int index
    required property string title
    required property string artist
    required property string duration
    required property bool playing
    required property url art

    signal play()
    signal remove()

    height: playing ? Metrics.queueNowRow : Metrics.queueRow

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: row.playing ? Theme.color.rowPlaying : hover.hovered ? Theme.color.stateHover : "transparent"
    }

    ArtTile {
        id: cover
        x: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.queueArt
        height: Metrics.queueArt
        radius: Metrics.radiusArtSmall
        source: row.art
        tint: row.index
    }

    Column {
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: tail.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter

        Text {
            width: parent.width
            text: row.title
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: row.playing ? Theme.color.accent : Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: row.artist
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }

    Item {
        id: tail
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.button24 + Metrics.gap8
        height: Metrics.button24

        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !row.playing && !hover.hovered
            text: row.duration
            font.pointSize: Type.small
            font.features: { "tnum": 1 }
            color: Theme.color.textTertiary
        }
        PlayingIndicator {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: row.playing
        }
        IconButton {
            anchors.right: parent.right
            visible: hover.hovered && !row.playing
            tip: I18n.tr("Remove from the queue", I18n.rev)
            glyph: Icons.x
            size: Metrics.button24
            onClicked: row.remove()
        }
    }

    HoverHandler { id: hover }
    TapHandler { onDoubleTapped: row.play() }
}
