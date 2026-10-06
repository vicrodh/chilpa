import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "../Components"

// Home (design/06): what played last, what came in last, and the most
// played tracks. Each shelf is the same AlbumGrid as every other grid,
// limited to one row.
Flickable {
    id: page

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    AlbumGridModel {
        id: played
        kind: "recentlyPlayed"
        contentRev: Library.contentRev
        limit: Metrics.shelfLimit
        maxRows: 1
        columns: playedGrid.columns
    }
    AlbumGridModel {
        id: added
        kind: "recentlyAdded"
        contentRev: Library.contentRev
        limit: Metrics.shelfLimit
        maxRows: 1
        columns: addedGrid.columns
    }
    TrackListModel {
        id: most
        kind: "mostPlayed"
        filterRev: Library.contentRev
        limit: Metrics.mostPlayedLimit
        playingPath: Player.path
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap8

        Text {
            text: I18n.tr("Home", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }

        Text {
            visible: played.albums === 0 && added.albums === 0
            topPadding: Metrics.section
            text: I18n.tr("Nothing here yet: add a folder to the library and its records show up here.", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        PillButton {
            visible: played.albums === 0 && added.albums === 0
            kind: "primary"
            glyph: Icons.folderPlus
            text: I18n.tr("Add a folder", I18n.rev)
            onClicked: Navigation.go("libraryFolders", 0)
        }

        SectionHeader {
            width: parent.width
            visible: played.albums > 0
            title: I18n.tr("Recently played", I18n.rev)
        }
        AlbumGrid {
            id: playedGrid
            width: parent.width
            height: implicitHeight
            visible: played.albums > 0
            fitContent: true
            model: played
            onOpen: (albumId) => Navigation.go("album", albumId)
        }

        SectionHeader {
            width: parent.width
            visible: added.albums > 0
            title: I18n.tr("Recently added", I18n.rev)
        }
        AlbumGrid {
            id: addedGrid
            width: parent.width
            height: implicitHeight
            visible: added.albums > 0
            fitContent: true
            model: added
            onOpen: (albumId) => Navigation.go("album", albumId)
        }

        SectionHeader {
            width: parent.width
            visible: most.count > 0
            title: I18n.tr("Most played", I18n.rev)
        }
        TrackList {
            width: parent.width
            height: implicitHeight
            visible: most.count > 0
            fitContent: true
            showPlays: true
            model: most
        }
    }
}
