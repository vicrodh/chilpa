import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Patterns"

// A playlist (design/06 Playlist): a mosaic of its first records, its name
// and length, play, shuffle or queue it, and its tracks in its order. A
// track's menu here can take it out of the playlist.
Flickable {
    id: page

    readonly property int playlistId: Navigation.routeId
    readonly property string playlistName: Playlists.name(playlistId, Playlists.rev)
    readonly property bool editable: Playlists.editable(playlistId, Playlists.rev)
    property string order: "custom"
    property bool ascending: true
    property bool selectionMode: false

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    TrackListModel {
        id: tracks
        kind: "playlist"
        albumId: page.playlistId
        // A source track's cover arrives after the list is drawn.
        filterRev: Playlists.rev + Library.contentRev + Library.remoteRev + Library.remoteArtRev
        playingPath: Player.path
        order: page.order
        ascending: page.ascending
        playlistEditable: page.editable
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        PageHero {
            width: parent.width
            id: hero
            label: I18n.tr("Playlist", I18n.rev) + " · " + Playlists.sourceName(page.playlistId, Playlists.rev)
            name: page.playlistName
            covers: Playlists.covers(page.playlistId, Playlists.rev)
            meta: tracks.count > 0 ? tracks.summary : I18n.tr("Empty. Add tracks from any list with a right click.", I18n.rev)
            markKind: "playlist"
            markKey: page.playlistId > 0 ? page.playlistId.toString() : ""
            canQueue: page.editable
            canPlay: page.editable
            queueGlyph: Icons.dotsVertical
            queueTip: I18n.tr("Queue and playlist actions", I18n.rev)
            canSelect: page.editable && tracks.count > 0
            selecting: page.selectionMode
            onPlay: tracks.playAll(false)
            onShuffle: tracks.playAll(true)
            onQueue: (left, top) => {
                const at = hero.mapToItem(page, left, top)
                playlistActions.show(at.x, at.y)
            }
            onSelectToggle: {
                page.selectionMode = !page.selectionMode
                tracks.clearSelection()
            }
        }

        Text {
            visible: Playlists.description(page.playlistId, Playlists.rev) !== ""
            width: parent.width
            text: Playlists.description(page.playlistId, Playlists.rev)
            wrapMode: Text.Wrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }

        Row {
            spacing: Metrics.gap8
            SortButton {
                id: sort
                value: page.ascending ? page.order : turned(page.order)
                options: [
                    {value: "custom", text: I18n.tr("Custom", I18n.rev), fixed: true},
                    {value: "title", text: I18n.tr("Title", I18n.rev)},
                    {value: "artist", text: I18n.tr("Artist", I18n.rev)},
                    {value: "album", text: I18n.tr("Album", I18n.rev)},
                    {value: "duration", text: I18n.tr("Duration", I18n.rev)},
                    {value: "plays", text: I18n.tr("Most played", I18n.rev), descending: true}
                ]
                onPicked: (value) => { page.order = baseOf(value); page.ascending = value === page.order }
            }
            Text {
                visible: page.order === "custom" && page.editable
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Drag or use ↑↓ to keep a custom order.", I18n.rev)
                font.pointSize: Type.meta
                color: Theme.color.textTertiary
            }
            Text {
                visible: page.order === "custom" && !page.editable
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("This source owns the order.", I18n.rev)
                font.pointSize: Type.meta
                color: Theme.color.textTertiary
            }
        }

        Text {
            visible: page.selectionMode && tracks.selectedCount === 0
            text: I18n.tr("Click tracks to select them; Shift selects a range.", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }

        TrackList {
            width: parent.width
            height: implicitHeight
            visible: tracks.count > 0
            fitContent: true
            model: tracks
            sortable: false
            order: page.order
            selectionMode: page.selectionMode
            reorderable: page.editable && page.order === "custom" && !page.selectionMode
        }
    }

    QueueMenu {
        id: playlistActions
        model: tracks
        laterLabel: page.playlistName
        canRemove: page.editable
        onRemove: Playlists.remove(page.playlistId)
    }

    PlaylistSelectionBar {
        visible: page.selectionMode && tracks.selectedCount > 0
        z: 2
        model: tracks
        playlistName: page.playlistName
        canRemove: page.editable
        x: (page.width - width) / 2
        y: page.contentY + page.height - height - Metrics.selectionBottom
        onFinished: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }

    // The playlist's own order is changed a place at a time.
    Shortcut {
        sequence: "Alt+Up"
        enabled: page.editable && page.order === "custom"
        onActivated: tracks.moveInPlaylist(tracks.selectedRow(), -1)
    }
    Shortcut {
        sequence: "Alt+Down"
        enabled: page.editable && page.order === "custom"
        onActivated: tracks.moveInPlaylist(tracks.selectedRow(), 1)
    }
}
