import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/artwork/qml/Components"
import "qrc:/qt/qml/app/chilpa/playlists/qml/Components" as Lists

// A record (design/06 Album): its header, the bar with discs and where the
// information comes from, and its tracks with a header per disc.
Flickable {
    id: page
    property double albumId: Navigation.routeId

    property bool selectionMode: false
    property bool presented: false
    Connections {
        target: page.Window.window
        function onFrameSwapped() { if (page.visible) page.presented = true }
    }
    function toggleSelection() { selectionMode = !selectionMode; tracks.clearSelection() }
    function showActions(item, left, top) { const at = item.mapToItem(page, left, top); albumMenu.active = true; albumMenu.item.show(detail.selectedId, detail.title, detail.artist, -1, at.x, at.y) }
    // The cover's own menu, with what it offers now.
    function showCoverMenu(item, left, top) {
        const at = item.mapToItem(page, left, top)
        coverMenu.active = true
        coverMenu.item.custom = Artwork.albumChosen(detail.selectedId, Artwork.rev)
        coverMenu.item.showing = detail.art.toString() !== ""
        coverMenu.item.address = Artwork.albumAddress(detail.selectedId, Artwork.rev)
        coverMenu.item.popupAt(at.x, at.y)
    }
    function showArtwork() { lightbox.active = true; lightbox.item.show(detail.art) }
    function showInformation() { information.active = true; information.item.open() }

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar { viewportHovered: pageHover.hovered }
    HoverHandler { id: pageHover; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad }

    AlbumDetailModel {
        id: detail
        albumId: page.albumId
        contentRev: Library.contentRev + I18n.rev
        remoteRev: Library.remoteRev + Library.remoteArtRev
    }
    TrackListModel {
        id: tracks
        objectName: "albumTracks"
        kind: "album"
        albumId: detail.selectedId
        albumPage: detail.tracksRev
        playingPath: Player.path
        // What is typed finds the record's rows here, with nothing asked of
        // the library or the source.
        key: find.typed
        // A source's playlist shows its tracks in the order kept for it.
        order: playlistSort.visible ? playlistSort.order : "custom"
        ascending: playlistSort.ascending
    }
    onAlbumIdChanged: find.clear()
    AlbumHeaderBackground {
        width: page.width
        height: Metrics.contentTop + (sourceDetails.visible ? sourceDetails.heroHeight : hero.height) + Metrics.gap16
        art: detail.art
        mode: SettingsModel.albumHeader
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        AlbumHero {
            id: hero
            visible: !sourceDetails.visible
            width: parent.width
            detail: detail
            selecting: page.selectionMode
            // White over the cover's darkened field, or over the ambient veil only where that veil is dark.
            contrasted: Theme.ambient ? Theme.dark : SettingsModel.albumHeader !== "off" && detail.art.toString() !== ""
            onPlay: tracks.playAll(false)
            onShuffle: tracks.playAll(true)
            onSelectToggle: page.toggleSelection()
            onContext: (left, top) => page.showActions(hero, left, top)
            onArtworkMenu: (left, top) => page.showCoverMenu(hero, left, top)
            onArtwork: page.showArtwork()
            onInformation: page.showInformation()
        }

        Loader {
            id: sourceDetails
            width: parent.width
            active: detail.selectedId < 0
            visible: active
            readonly property var info: item ? item.info : ({})
            readonly property real heroHeight: item ? item.heroHeight : 0
            sourceComponent: SourceRecordDetails {
                width: parent.width; cardId: detail.selectedId; trackModel: tracks; record: detail
                selecting: page.selectionMode
                onArtwork: page.showArtwork()
                onInformation: page.showInformation()
                onSelectToggle: page.toggleSelection()
                onContext: (left, top) => page.showActions(sourceDetails, left, top)
                onArtworkMenu: (left, top) => page.showCoverMenu(sourceDetails, left, top)
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }

        Item {
            width: parent.width
            height: Math.max(Metrics.gap16, find.height)

            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap16

                Row {
                    visible: detail.discs !== ""
                    spacing: Metrics.gap6
                    Icon { glyph: Icons.disc; size: Metrics.iconSm }
                    Text { text: detail.discs; font.pointSize: Type.meta; color: Theme.color.textSecondary }
                }
                Row {
                    spacing: Metrics.gap6
                    Icon { glyph: Icons.infoCircle; size: Metrics.iconSm; color: Theme.color.textTertiary }
                    Text { text: detail.info; font.pointSize: Type.meta; color: Theme.color.textTertiary }
                }
            }

            Lists.PlaylistSortButton {
                id: playlistSort
                visible: sourceDetails.info.kind === "playlist"
                playlistId: visible ? detail.selectedId : 0
                anchors.right: find.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
            }
            SettledSearchField {
                id: find
                objectName: "albumTrackSearch"
                anchors.right: summaryText.left
                anchors.rightMargin: Metrics.gap16
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.screenSearch
            }
            Text {
                id: summaryText
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: detail.summary
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }

        TrackList {
            width: parent.width
            height: implicitHeight
            fitContent: true
            viewport: page
            prefetch: page.presented
            showArtist: sourceDetails.info.kind === "playlist"
            showAlbum: sourceDetails.info.kind === "playlist"
            showArt: sourceDetails.info.kind === "playlist"
            model: tracks
            selectionMode: page.selectionMode
        }

        Loader {
            objectName: "albumRecommendationsLoader"
            width: parent.width
            height: item ? item.implicitHeight : 1
            active: page.presented && detail.found
            asynchronous: true
            property double albumId: detail.selectedId
            property Flickable viewport: page
            property var shelves: (sourceDetails.info.shelves || []).filter(shelf => !(shelf.id === "tracks" && shelf.kind === "track"))
            source: "qrc:/qt/qml/app/chilpa/sources/qml/Patterns/AlbumRecommendations.qml"
        }
    }

    Loader { id: albumMenu; objectName: "albumActionMenuLoader"; active: false; sourceComponent: AlbumMenu {} }
    Loader { id: lightbox; active: false; sourceComponent: CoverLightbox {} }
    Loader {
        id: coverMenu
        active: false
        sourceComponent: CoverMenu {
            onChoose: Artwork.chooseAlbum(detail.selectedId)
            onForget: Artwork.forgetAlbum(detail.selectedId)
            onView: page.showArtwork()
            onSave: Artwork.saveAlbum(detail.selectedId, detail.title)
        }
    }
    Loader {
        id: information
        objectName: "albumInformationLoader"
        active: false
        sourceComponent: AlbumInfoModal { albumId: detail.selectedId; art: detail.art; summary: detail.summary }
    }
    Loader {
        active: page.selectionMode && tracks.selectedCount > 0
        z: 2
        x: (page.width - width) / 2
        y: page.contentY + page.height - height - Metrics.selectionBottom
        sourceComponent: PlaylistSelectionBar {
            model: tracks
            playlistName: detail.title
            canRemove: false
            onFinished: {
                tracks.clearSelection()
                page.selectionMode = false
            }
        }
    }
}
