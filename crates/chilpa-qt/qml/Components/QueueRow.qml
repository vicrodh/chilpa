// hot-path: one per queued track, recycled while the queue scrolls.
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A track in the side queue (design/03 QueueRow): the grid
// 12 | 36 | 1fr | auto | 20 -- grip, cover, title and artist, time and the
// item's menu. The grip's column is kept and its glyph is not drawn until
// the queue can be reordered by dragging. The one that plays is 48 high,
// raised, with the playing bars where the time goes; its words keep their
// colours, as in QBZ, so they read over the dynamic background. A double click
// plays from here; the menu, or a right click, has the rest.
Item {
    id: row

    required property int index
    required property string title
    required property string artist
    required property string duration
    required property bool playing
    required property url art

    signal play()
    signal menu(real x, real y)

    height: playing ? Metrics.queueNowRow : Metrics.queueRow

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        radius: Metrics.radiusCard
        hovered: hover.hovered
        emphasized: row.playing
    }

    ArtTile {
        id: cover
        x: Metrics.gap4 + Metrics.queueGrip
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
        anchors.right: time.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter

        Text {
            width: parent.width
            text: row.title
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: row.artist
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }

    Text {
        id: time
        anchors.right: more.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        visible: !row.playing
        text: row.duration
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    PlayingIndicator {
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        visible: row.playing
        running: Player.playing && !Player.paused
    }

    IconButton {
        id: more
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        visible: !row.playing
        size: Metrics.queueMore
        glyph: Icons.dotsVertical
        tip: I18n.tr("More", I18n.rev)
        onClicked: row.menu(x, y + height)
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onDoubleTapped: row.play() }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: (point) => row.menu(point.position.x, point.position.y)
    }
}
