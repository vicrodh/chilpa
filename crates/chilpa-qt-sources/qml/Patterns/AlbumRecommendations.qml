import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// I load these shelves independently of a record's header and tracks.
Column {
    id: recommendations
    property double albumId: parent.albumId || 0
    property Flickable viewport: parent.viewport || null
    property var shelves: parent.shelves || []
    spacing: Metrics.gap16
    KeyedModel { id: sourceShelves; items: recommendations.shelves }
    AlbumGridModel {
        id: related
        kind: "related"
        key: String(recommendations.albumId)
        limit: 8
        contentRev: Library.contentRev
    }
    SourceShelf {
        viewport: recommendations.viewport
        width: parent.width
        modelKind: "sameArtist"
        section: ({title: I18n.tr("From same artist", I18n.rev), key: String(recommendations.albumId), size: 18})
    }
    Repeater {
        model: sourceShelves
        SourceShelf { required property var entry; viewport: recommendations.viewport; width: recommendations.width; section: entry }
    }
    AlbumCarousel { viewport: recommendations.viewport; width: parent.width; title: I18n.tr("Similar albums", I18n.rev); model: related }
}
