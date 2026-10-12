import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The scene's light album card has a 220px cover and two actions.
Item {
    id: card
    property var album: ({})
    property string plugin: ""
    readonly property int albumId: plugin && /^album:[^:]+$/.test(album.ref || "") ? Sources.referenceCard(plugin, album.ref) : 0
    width: Metrics.scenePanelAlbumWidth; height: Metrics.scenePanelCardHeight
    ArtTile {
        id: cover
        width: parent.width; height: width; radius: Metrics.radiusCard
        source: card.plugin && card.album.image ? Sources.art(card.plugin, card.album.image, Library.remoteArtRev) : ""
    }
    Rectangle {
        anchors.fill: cover; visible: hover.hovered && card.albumId !== 0
        radius: Metrics.radiusCard; color: Theme.color.overlayScrim
        Row {
            anchors.centerIn: parent; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Play", I18n.rev); onClicked: AlbumActions.play(card.albumId, false) }
            PillButton { text: I18n.tr("Open", I18n.rev); onClicked: Navigation.go("album", card.albumId) }
        }
    }
    Text {
        y: cover.height + Metrics.gap4; width: parent.width
        text: card.album.title || ""; textFormat: Text.PlainText; elide: Text.ElideRight
        font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary
    }
    Text {
        y: cover.height + Metrics.gap4 + Metrics.gap22; width: parent.width
        text: [card.album.artist || "", card.album.year || ""].filter(Boolean).join(" · ")
        textFormat: Text.PlainText; elide: Text.ElideRight
        font.pointSize: Type.meta; color: Theme.color.textSecondary
    }
    HoverHandler { id: hover; cursorShape: Qt.ArrowCursor }
}
