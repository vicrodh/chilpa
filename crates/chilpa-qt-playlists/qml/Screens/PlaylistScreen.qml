import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "../Components"
import "../Patterns"

// A playlist of the library, laid out as a source's playlist is on its
// record page: the header over its cover, the bar between two rules with
// its order, what is typed to find a row and how much it holds, then its
// tracks. A track's menu here can take it out of the playlist.
Flickable {
    id: page

    property int playlistId: Navigation.routeId
    // A server answers after the change was asked: the library's turn too.
    readonly property int rev: Playlists.rev + Library.contentRev
    readonly property string playlistName: Playlists.name(playlistId, page.rev)
    readonly property bool editable: Playlists.editable(playlistId, Playlists.rev)
    // A music server's own playlist: it plays as a whole, and its tracks are
    // changed through the server, while the account may and nothing else of
    // it is on its way there.
    readonly property var server: JSON.parse(PlaylistManager.serverPlaylist(playlistId, page.rev) || "{}")
    readonly property bool ofServer: server.icon !== undefined
    readonly property bool changeable: editable || (ofServer && server.writable === true && server.pending !== true)
    readonly property var covers: Playlists.covers(playlistId, Playlists.rev)
    // The order the playlist keeps for its tracks, chosen with its sort button.
    readonly property string order: sort.order
    readonly property bool ascending: sort.ascending
    property bool selectionMode: false
    // White only over the cover's darkened field. On the dynamic background a
    // playlist reads in the theme's own colors, as QBZ's playlist page does.
    readonly property bool contrasted: SettingsModel.albumHeader !== "off" && page.covers.length > 0 && !Theme.ambient

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}
    onPlaylistIdChanged: find.clear()

    TrackListModel {
        id: tracks
        objectName: "playlistTracks"
        kind: "playlist"
        albumId: page.playlistId
        // A source track's cover arrives after the list is drawn.
        filterRev: Playlists.rev + Library.contentRev + Library.remoteRev + Library.remoteArtRev
        playingPath: Player.path
        order: page.order
        ascending: page.ascending
        playlistEditable: page.changeable
        key: find.typed
    }
    AlbumHeaderBackground {
        width: page.width
        height: Metrics.contentTop + hero.height + Metrics.gap16
        art: page.covers.length > 0 ? page.covers[0] : ""
        mode: SettingsModel.albumHeader
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        PlaylistHeader {
            id: hero
            width: parent.width
            name: page.playlistName
            // The library's own lists are Chilpa's, as the Home's cards name them.
            sourceName: page.editable ? "Chilpa" : Playlists.sourceName(page.playlistId, Playlists.rev)
            sourceIcon: page.editable ? "qrc:/qt/qml/app/chilpa/kit/assets/logo.png" : page.server.icon || ""
            covers: page.covers
            meta: (tracks.count > 0 || find.typed !== "" ? tracks.summary : I18n.tr("Empty. Add tracks from any list with a right click.", I18n.rev))
                + (page.server.left_out > 0 ? " · " + I18n.trCount("{} track on the server is not in your library", "{} tracks on the server are not in your library", page.server.left_out, I18n.rev) : "")
            description: Playlists.description(page.playlistId, page.rev)
            markKey: page.playlistId > 0 ? page.playlistId.toString() : ""
            canPlay: page.editable || page.ofServer
            canSelect: page.changeable && tracks.count > 0
            selecting: page.selectionMode
            contrasted: page.contrasted
            onPlay: tracks.playAll(false)
            onShuffle: tracks.playAll(true)
            onContext: (left, top) => {
                const at = hero.mapToItem(page, left, top)
                playlistActions.show(at.x, at.y)
            }
            onSelectToggle: {
                page.selectionMode = !page.selectionMode
                tracks.clearSelection()
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }

        Item {
            width: parent.width
            height: Math.max(Metrics.gap16, find.height)
            Row {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.right: sort.left
                anchors.rightMargin: Metrics.gap16
                spacing: Metrics.gap6
                visible: hint.text !== ""
                Icon { glyph: Icons.infoCircle; size: Metrics.iconSm; color: Theme.color.textTertiary }
                Text {
                    id: hint
                    width: parent.width - Metrics.iconSm - parent.spacing
                    elide: Text.ElideRight
                    text: page.server.pending === true ? I18n.trArgs("Sending the change to {}…", [page.server.server || ""], I18n.rev)
                        : page.selectionMode && tracks.selectedCount === 0 ? I18n.tr("Click tracks to select them; Shift selects a range.", I18n.rev)
                        : page.order !== "custom" || find.typed !== "" ? ""
                        : page.changeable ? I18n.tr("Drag or use ↑↓ to keep a custom order.", I18n.rev) : I18n.tr("This source owns the order.", I18n.rev)
                    font.pointSize: Type.meta
                    color: Theme.color.textTertiary
                }
            }
            PlaylistSortButton {
                id: sort
                playlistId: page.playlistId
                anchors.right: find.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
            }
            SettledSearchField {
                id: find
                objectName: "playlistTrackSearch"
                anchors.right: summary.left
                anchors.rightMargin: Metrics.gap16
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.screenSearch
            }
            Text {
                id: summary
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: tracks.summary
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }

        TrackList {
            width: parent.width
            height: implicitHeight
            visible: tracks.count > 0
            fitContent: true
            viewport: page
            model: tracks
            sortable: false
            order: page.order
            selectionMode: page.selectionMode
            // Rows found by what is typed are not the whole order to move in.
            reorderable: page.changeable && page.order === "custom" && !page.selectionMode && find.typed === ""
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
        canRemove: page.changeable
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
        enabled: !InputLayers.blocksShortcuts(page.Window.window) && (page.changeable && page.order === "custom" && find.typed === "")
        onActivated: tracks.moveInPlaylist(tracks.selectedRow(), -1)
    }
    Shortcut {
        sequence: "Alt+Down"
        enabled: !InputLayers.blocksShortcuts(page.Window.window) && (page.changeable && page.order === "custom" && find.typed === "")
        onActivated: tracks.moveInPlaylist(tracks.selectedRow(), 1)
    }
}
