import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// I present a recommendation's seed with the library's full card behavior.
Item {
    id: context
    property string key: ""
    implicitHeight: grid.implicitHeight
    AlbumGridModel {
        id: record
        kind: "source"; key: context.key; limit: 1; columns: 1
        contentRev: Library.remoteRev + Library.remoteArtRev
    }
    AlbumGrid {
        id: grid
        width: CardLayout.cardWidth + Metrics.cardRing * 2 // derived: the library cover with room for its ring
        height: implicitHeight; fitContent: true; fixedColumns: 1
        viewportWidth: context.width
        model: record
        onOpen: albumId => Navigation.go("album", albumId)
    }
}
