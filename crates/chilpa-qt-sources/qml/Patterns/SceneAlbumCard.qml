import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// I use the same cover size, metadata and actions as every album card.
AlbumCard {
    id: card
    property var album: ({})
    property string plugin: ""
    albumId: plugin && /^album:[^:\s]+(?::[^:\s]+)*$/.test(album.ref || "") ? Sources.referenceCard(plugin, album.ref) : 0
    width: CardLayout.cardWidth
    height: implicitHeight
    title: album.title || ""
    subtitle: [album.artist || "", album.year || ""].filter(Boolean).join(" · ")
    artistName: album.artist || ""
    artistDestination: artistName && plugin ? ({target:"source",key:plugin + " " + (album.artist_ref ? "native:" + album.artist_ref : "matching-artist-v3:" + artistName)}) : ({})
    year: String(album.year || "")
    art: plugin && album.image ? Sources.art(plugin, album.image, Library.remoteArtRev) : ""
    sources: albumId ? JSON.parse(AlbumActions.sources(albumId, Library.remoteRev) || "[]") : []
    qualityInfo: albumId ? JSON.parse(AlbumActions.quality(albumId, Library.remoteRev) || "{}") : ({})
    available: albumId !== 0 && AlbumActions.available(albumId, Library.remoteRev)
    canExpand: false
    onClicked: if (albumId) Navigation.go("album", albumId)
    onMenu: (left, top) => {
        const at = card.mapToItem(null, left, top)
        cardMenu.show(albumId, title, album.artist || "", -1, at.x, at.y)
    }
    AlbumMenu { id: cardMenu }
}
