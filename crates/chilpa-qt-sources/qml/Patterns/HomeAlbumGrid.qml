import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"

// A Home rail of records in the compact grid (QBZ's SlimCard): a click opens
// the record, a right click its menu.
SlimGrid {
    id: albums
    property AlbumGridModel model
    total: model ? model.albums : 0
    cells: Repeater {
        model: albums.shown
        SlimRow {
            required property int index
            readonly property int revision: albums.model.rev
            readonly property int albumId: albums.model.albumId(index, revision)
            x: albums.cellX(index); y: albums.cellY(index)
            width: albums.cellWidth; height: Metrics.slimRow
            art: albums.model.art(index, revision)
            title: albums.model.title(index, revision)
            subtitle: albums.model.subtitle(index, revision)
            onActivated: Navigation.go("album", albumId)
            onMenuAsked: (x, y) => {
                const point = mapToItem(albums, x, y)
                menu.show(albumId, title, subtitle, index, point.x, point.y)
            }
        }
    }
    AlbumMenu { id: menu }
}
