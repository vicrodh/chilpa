import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Components"

// Artists (design/06): everyone in the library, as round portraits in the
// shared grid. A portrait is a cover of one of their records.
Item {
    id: page

    AlbumGridModel {
        id: everyone
        kind: "artists"
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
            text: I18n.tr("Artists", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.trCount("{} artist", "{} artists", everyone.albums, I18n.rev)
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
        round: true
        bottomInset: Metrics.contentBottom
        model: everyone
        onOpen: (albumId, title) => Navigation.openArtist(title)
    }
}
