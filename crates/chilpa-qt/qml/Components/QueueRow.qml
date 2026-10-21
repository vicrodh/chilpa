// hot-path: one per queued track, recycled while the queue scrolls.
import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// I show each queued track with a grip, cover, title, time and menu.
// The playing row is raised and puts its bars where the time was.
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
        sceneAnimating: Ambient.mode === 1 || Ambient.mode === 2 || Ambient.mode === 4
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        visible: row.playing
        running: Player.playing && !Player.paused; onToggled: Player.toggle()
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
