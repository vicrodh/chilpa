import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// A record of the results page, drawn as a record is everywhere else
// (AlbumCard, as QBZ's search draws its own): the cover wearing whose it
// is, the title, the artist and the quality, and under the pointer the
// year, genre, play, favorite and menu. A source's record has its catalog
// id from when its source answered; one without an id is dimmed.
AlbumCard {
    id: record
    property var row: ({})
    readonly property int rev: Library.contentRev + Library.remoteRev
    width: CardLayout.cardWidth
    albumId: row.albumId || 0
    title: row.title || ""
    subtitle: row.artist || ""
    year: row.year || ""
    art: !row.art ? "" : row.art.startsWith("https://") ? row.art : "file://" + row.art.split("/").map(encodeURIComponent).join("/")
    // Results of every source mix here, so the library's own wear Chilpa's mark.
    sources: albumId > 0 ? [{local: true, chilpa: true, name: "Chilpa"}]
        : albumId < 0 ? JSON.parse(AlbumActions.sources(albumId, rev) || "[]")
        : (row.sourceIcon || "") !== "" ? [{icon: row.sourceIcon, name: row.sourceName}] : []
    qualityInfo: albumId !== 0 ? JSON.parse(AlbumActions.quality(albumId, rev) || "{}") : ({})
    canExpand: false
    available: albumId !== 0 && AlbumActions.available(albumId, rev)
}
