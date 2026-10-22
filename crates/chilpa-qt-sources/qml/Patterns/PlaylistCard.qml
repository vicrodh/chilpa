import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A playlist as a card, on the Home's shelves and the results page: its
// cover, or a mosaic of its tracks' covers, play, its name and whose it is,
// with the source's icon or Chilpa's mark for the library's own.
Column {
    id: card
    property string name
    // A cover on disk or at an address, or none.
    property string art
    property var covers: []
    // Whose it is, as SourceIcon takes it, and its name.
    property var origin: ({})
    property string owner
    property bool canPlay: true
    signal activated()
    signal played()
    signal menuAsked(real x, real y)
    width: CardLayout.cardWidth
    height: CardLayout.cardHeight
    spacing: Metrics.gap4

    Item {
        width: parent.width; height: width
        ArtTile {
            id: cover; anchors.fill: parent
            source: !card.art ? "" : card.art.indexOf("://") >= 0 ? card.art : "file://" + card.art.split("/").map(encodeURIComponent).join("/")
        }
        PlaylistCollage {
            anchors.fill: parent; visible: !cover.ready && card.covers.length > 0
            covers: card.covers
        }
        Icon {
            anchors.centerIn: parent; visible: !cover.ready && card.covers.length === 0
            glyph: Icons.playlist; size: Metrics.iconPlay; color: Theme.color.textSecondary
        }
        MouseArea {
            anchors.fill: parent; cursorShape: Qt.PointingHandCursor
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: (mouse) => mouse.button === Qt.RightButton ? card.menuAsked(mouse.x, mouse.y) : card.activated()
        }
        IconButton {
            objectName: "playlistCardPlay"
            visible: card.canPlay
            anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: Metrics.gap8
            round: true; glyph: Icons.playerPlayFilled; filled: true
            tip: I18n.tr("Play", I18n.rev)
            onClicked: card.played()
        }
    }
    Text { width: parent.width; text: card.name; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
    Row {
        id: ownerLine
        width: parent.width; spacing: Metrics.gap4
        SourceIcon {
            id: originIcon
            objectName: "playlistCardOrigin"
            visible: card.origin.chilpa === true || (card.origin.icon || "") !== ""
            sourceInfo: card.origin
            width: visible ? Metrics.iconSm : 0; height: Metrics.iconSm
        }
        Text { width: ownerLine.width - originIcon.width - ownerLine.spacing; text: card.owner; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
    }
}
