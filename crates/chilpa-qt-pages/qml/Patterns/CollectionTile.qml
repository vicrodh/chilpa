import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Item {
    id: tile
    required property var entry
    required property string plugin
    property bool active: true
    property bool favorites: false
    property bool playable: false
    property bool favoriteBusy: false
    signal opened()
    signal playRequested()
    signal menuRequested(real x, real y)
    signal favoriteRequested(bool on)
    implicitHeight: card.implicitHeight
    AlbumCard {
        id: card
        width: parent.width
        albumId: tile.entry._host?.card_id || 0
        title: tile.entry.title || ""
        subtitle: tile.entry.artist || tile.entry.subtitle || tile.entry.label || ""
        art: tile.active ? (NativePages.coverOverride(tile.entry._host?.card_id || 0, Library.remoteArtRev) || Sources.art(tile.plugin, tile.entry.image || "", Library.remoteArtRev)) : ""
        round: tile.entry.kind === "artist"
        available: tile.entry.available !== false
        purchaseMode: true
        contextMenuEnabled: true
        canExpand: false
        containArt: tile.entry.art_fit === "contain"
        onClicked: tile.opened()
        onMenu: (x,y) => tile.menuRequested(x,y)
    }
    HoverHandler { id: pointer }
    IconButton {
        objectName: "collectionFavorite"
        anchors.top: parent.top; anchors.right: parent.right
        anchors.margins: Metrics.gap8
        visible: tile.favorites && (pointer.hovered || checked)
        checked: tile.entry.favorite === true
        filled: checked
        glyph: checked ? Icons.heartFilled : Icons.heart
        heart: true; round: true; tone: "overlay"
        busy: tile.favoriteBusy
        tip: checked ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev)
        onClicked: tile.favoriteRequested(!checked)
    }
    IconButton {
        objectName: "collectionPlay"
        anchors.right: parent.right
        y: tile.width - height - Metrics.gap8
        anchors.rightMargin: Metrics.gap8
        visible: pointer.hovered && tile.playable && tile.entry.available !== false
        glyph: Icons.playerPlayFilled; filled:true; round:true; tone:"solid"
        tip: I18n.tr("Play now", I18n.rev)
        onClicked: tile.playRequested()
    }
}
