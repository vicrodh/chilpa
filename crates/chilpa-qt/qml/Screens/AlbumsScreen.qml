import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Components"

// Albums (design/06): every record in the library, in one grid. The grid
// scrolls over a snapshot of ids and reads its cards a page at a time, so
// ten thousand records scroll like ten.
Item {
    id: page

    AlbumGridModel {
        id: all
        kind: "all"
        order: "artist"
        contentRev: Library.contentRev
        columns: grid.columns
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Albums", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.trCount("{} album", "{} albums", all.albums, I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    AlbumGrid {
        id: grid
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        bottomInset: Metrics.contentBottom
        model: all
        onOpen: (albumId) => Navigation.go("album", albumId)
    }
}
