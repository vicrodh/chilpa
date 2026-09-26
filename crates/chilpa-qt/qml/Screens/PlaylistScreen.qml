import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// A playlist (design/06 Playlist): a mosaic of its first records, its name
// and length, play, shuffle or queue it, and its tracks in its order. A
// track's menu here can take it out of the playlist.
Flickable {
    id: page

    readonly property int playlistId: Navigation.routeId

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}

    TrackListModel {
        id: tracks
        kind: "playlist"
        albumId: page.playlistId
        filterRev: Playlists.rev + Library.contentRev
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
            label: I18n.tr("Playlist", I18n.rev)
            name: Playlists.name(page.playlistId, Playlists.rev)
            covers: Playlists.covers(page.playlistId, Playlists.rev)
            meta: tracks.count > 0 ? tracks.summary : I18n.tr("Empty. Add tracks from any list with a right click.", I18n.rev)
            canQueue: true
            onPlay: tracks.playAll(false)
            onShuffle: tracks.playAll(true)
            onQueue: tracks.queueAll()
        }

        Text {
            visible: tracks.count > 1
            text: I18n.tr("Select a track and use Alt ↑↓, or its menu, to move it.", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }

        TrackList {
            width: parent.width
            height: implicitHeight
            visible: tracks.count > 0
            fitContent: true
            model: tracks
        }
    }

    // The playlist's own order is changed a place at a time.
    Shortcut {
        sequence: "Alt+Up"
        onActivated: tracks.moveInPlaylist(tracks.selectedRow(), -1)
    }
    Shortcut {
        sequence: "Alt+Down"
        onActivated: tracks.moveInPlaylist(tracks.selectedRow(), 1)
    }
}
