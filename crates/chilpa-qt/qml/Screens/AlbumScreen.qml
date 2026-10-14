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

// A record (design/06 Album): its header, the bar with discs and where the
// information comes from, and its tracks with a header per disc.
Flickable {
    id: page
    property double albumId: Navigation.routeId

    property bool selectionMode: false
    KeyedModel {
        id: sourceShelves
        items: (sourceDetails.info.shelves || []).filter(shelf => !(shelf.id === "tracks" && shelf.kind === "track"))
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
        contentRev: Library.contentRev + Library.remoteRev + Library.remoteArtRev
    }
    TrackListModel {
        id: tracks
        kind: "album"
        albumId: detail.selectedId
        filterRev: Library.contentRev + Library.remoteRev + Library.remoteArtRev
        playingPath: Player.path
    }
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
            contrasted: SettingsModel.albumHeader !== "off" && detail.art.toString() !== ""
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
            height: Metrics.gap16

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

            Text {
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
            showArtist: sourceDetails.info.kind === "playlist"
            showAlbum: sourceDetails.info.kind === "playlist"
            showArt: sourceDetails.info.kind === "playlist"
            model: tracks
            selectionMode: page.selectionMode
        }

        Loader {
            width: parent.width
            active: page.visible && detail.found
            asynchronous: true
            property double albumId: detail.selectedId
            property var shelves: sourceShelves.items
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
