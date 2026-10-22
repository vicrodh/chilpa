import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A playlist as a card, the same wherever one is shown: the Home's shelves,
// the results page and the playlist manager. It reads as a record's card
// (AlbumCard): the cover, or a mosaic of its tracks' covers, wearing whose
// it is on its corner; its name and a line under it; and under the pointer
// the scrim with play and its menu. A folder is the same card with a folder
// where the cover goes, and no play.
Item {
    id: card
    property string name
    property string detail
    // A cover on disk or at an address, or none, and the tracks' covers on disk.
    property string art
    property var covers: []
    // Whose it is, as SourceIcon takes it: a source's icon or Chilpa's mark.
    property var origin: ({})
    property bool folder: false
    property color folderColor: Theme.color.textSecondary
    property bool canPlay: !folder
    property bool hasMenu: true
    signal activated()
    signal played()
    signal menuAsked(real x, real y)
    width: CardLayout.cardWidth
    implicitHeight: width + Metrics.cardTextGap + Metrics.cardTextHeight
    height: implicitHeight

    ArtTile {
        id: cover
        width: card.width; height: width
        radius: Metrics.radiusCard
        visible: !card.folder
        source: !card.art ? "" : card.art.indexOf("://") >= 0 ? card.art : "file://" + card.art.split("/").map(encodeURIComponent).join("/")
    }
    PlaylistCollage {
        anchors.fill: cover
        visible: !card.folder && !cover.ready && card.covers.length > 0
        covers: card.covers
    }
    Rectangle {
        anchors.fill: cover
        visible: card.folder || (!cover.ready && card.covers.length === 0)
        radius: Metrics.radiusCard
        color: Theme.color.fillField
        Icon {
            anchors.centerIn: parent
            glyph: card.folder ? Icons.folder : Icons.playlist
            size: Metrics.iconPlay
            color: card.folder ? card.folderColor : Theme.color.textSecondary
        }
    }

    // Whose it is, on the cover's corner as a record's sources are.
    Item {
        objectName: "playlistCardOrigin"
        readonly property bool shown: card.origin.chilpa === true || (card.origin.icon || "") !== ""
        width: badge.width; height: badge.height
        anchors.right: cover.right; anchors.bottom: cover.bottom; anchors.margins: Metrics.gap4
        visible: shown
        Rectangle {
            anchors.fill: badge
            anchors.margins: -Metrics.gap2
            radius: Metrics.radiusCard
            color: Theme.color.shadowElevatedKey
            opacity: 0.7
        }
        SourceIcon {
            id: badge
            width: Metrics.iconSm + Metrics.gap4; height: width
            sourceInfo: card.origin
            color: Theme.color.backdropText
            opacity: 0.84
        }
    }

    Loader {
        anchors.fill: cover
        active: hover.hovered && (card.canPlay || card.hasMenu)
        sourceComponent: Rectangle {
            objectName: "playlistCardOverlay"
            radius: Metrics.radiusCard
            color: Theme.color.overlayScrim
            Item {
                anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
                anchors.leftMargin: Metrics.overlayPad; anchors.rightMargin: Metrics.overlayPad
                anchors.bottomMargin: Metrics.overlayPad + Metrics.iconSm + Metrics.gap4 + Metrics.gap6 // derived: above the origin's badge, as a record's overlay
                height: Metrics.overlayPlay
                Rectangle {
                    objectName: "playlistCardPlay"
                    visible: card.canPlay
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    width: Metrics.overlayPlay; height: width
                    radius: width / 2 // derived: a circle
                    color: play.containsMouse ? Theme.color.accentHover : Theme.color.accent
                    Icon { anchors.centerIn: parent; glyph: Icons.playerPlayFilled; filled: true; size: Metrics.iconNav; color: Theme.color.textOnAccent }
                    MouseArea {
                        id: play
                        anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                        onClicked: card.played()
                        onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Play", I18n.rev), parent) : TipState.hide(parent)
                    }
                }
                IconButton {
                    id: more
                    objectName: "playlistCardMore"
                    visible: card.hasMenu
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    size: Metrics.overlayButton; round: true; tone: "overlay"
                    glyph: Icons.dots
                    tip: I18n.tr("More", I18n.rev)
                    onClicked: { const at = more.mapToItem(card, 0, more.height); card.menuAsked(at.x, at.y) }
                }
            }
        }
    }

    Column {
        anchors.top: cover.bottom
        anchors.topMargin: Metrics.cardTextGap
        width: card.width
        Text {
            objectName: "playlistCardName"
            width: parent.width
            text: card.name; textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.body; font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: card.detail; textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler {
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => button === Qt.RightButton ? (card.hasMenu && card.menuAsked(point.position.x, point.position.y)) : card.activated()
    }
}
