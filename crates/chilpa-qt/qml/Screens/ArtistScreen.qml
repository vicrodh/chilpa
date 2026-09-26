import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// An artist (design/06 Artist): the portrait and what the library has of
// them, their most played tracks, their records, newest first, and the
// compilations they are on.
Flickable {
    id: page

    readonly property string name: Navigation.routeKey

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}

    AlbumGridModel {
        id: records
        kind: "artist"
        key: page.name
        contentRev: Library.contentRev
        columns: grid.columns
    }
    AlbumGridModel {
        id: guest
        kind: "appearsOn"
        key: page.name
        contentRev: Library.contentRev
        columns: guestGrid.columns
    }
    TrackListModel {
        id: every
        kind: "artist"
        key: page.name
        filterRev: Library.contentRev
    }
    TrackListModel {
        id: top
        kind: "artistTop"
        key: page.name
        filterRev: Library.contentRev
        playingPath: Player.path
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        PageHero {
            width: parent.width
            label: I18n.tr("Artist", I18n.rev)
            round: true
            name: page.name
            art: records.albums > 0 ? records.art(0, records.rev) : guest.art(0, guest.rev)
            meta: records.albums > 0 ? I18n.trCount("{} record", "{} records", records.albums, I18n.rev) + " · " + every.summary
                : every.summary
            onPlay: every.playAll(false)
            onShuffle: every.playAll(true)
        }

        SectionHeader { width: parent.width; visible: top.count > 0; title: I18n.tr("Most played", I18n.rev) }
        TrackList {
            width: parent.width
            height: implicitHeight
            visible: top.count > 0
            fitContent: true
            showArtist: false
            showPlays: true
            model: top
        }

        SectionHeader { width: parent.width; visible: records.albums > 0; title: I18n.tr("Records", I18n.rev) }
        AlbumGrid {
            id: grid
            width: parent.width
            height: implicitHeight
            visible: records.albums > 0
            fitContent: true
            model: records
            artistPage: records.key
            onOpen: (albumId) => Navigation.go("album", albumId)
        }

        SectionHeader { width: parent.width; visible: guest.albums > 0; title: I18n.tr("Appears on", I18n.rev) }
        AlbumGrid {
            id: guestGrid
            width: parent.width
            height: implicitHeight
            visible: guest.albums > 0
            fitContent: true
            model: guest
            onOpen: (albumId) => Navigation.go("album", albumId)
        }
    }
}
