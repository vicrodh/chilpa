import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Patterns"

// A record (design/06 Album): its header, the bar with discs and where the
// information comes from, and its tracks with a header per disc.
Flickable {
    id: page

    property bool selectionMode: false
    function toggleSelection() { selectionMode = !selectionMode; tracks.clearSelection() }
    function showActions(item, left, top) { const at = item.mapToItem(page, left, top); albumMenu.show(detail.selectedId, detail.title, detail.artist, -1, at.x, at.y) }

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    AlbumDetailModel {
        id: detail
        albumId: Navigation.routeId
        contentRev: Library.contentRev + Library.remoteRev + Library.remoteArtRev
    }
    TrackListModel {
        id: tracks
        kind: "album"
        albumId: detail.selectedId
        filterRev: Library.contentRev + Library.remoteRev
        playingPath: Player.path
    }
    AlbumGridModel {
        id: sameArtist
        kind: "sameArtist"
        key: detail.selectedId.toString()
        limit: 8
        contentRev: Library.contentRev + Library.remoteRev + Library.remoteArtRev
    }
    AlbumGridModel {
        id: related
        kind: "related"
        key: detail.selectedId.toString()
        limit: 8
        contentRev: Library.contentRev
    }

    AlbumHeaderBackground {
        width: page.width
        height: Metrics.contentTop + (detail.selectedId < 0 ? sourceDetails.heroHeight : hero.height) + Metrics.gap16
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
            visible: detail.selectedId >= 0
            width: parent.width
            detail: detail
            selecting: page.selectionMode
            contrasted: SettingsModel.albumHeader !== "off" && detail.art.toString() !== ""
            onPlay: tracks.playAll(false)
            onShuffle: tracks.playAll(true)
            onSelectToggle: page.toggleSelection()
            onContext: (left, top) => page.showActions(hero, left, top)
            onArtwork: artwork.show(detail.art)
        }

        SourceRecordDetails { id: sourceDetails; width: parent.width; cardId: detail.selectedId; trackModel: tracks; record: detail
            selecting: page.selectionMode
            onArtwork: artwork.show(detail.art)
            onSelectToggle: page.toggleSelection()
            onContext: (left, top) => page.showActions(sourceDetails, left, top)
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
            showArtist: false
            showAlbum: false
            showArt: false
            model: tracks
            selectionMode: page.selectionMode
        }

        Repeater { model: sourceDetails.info.shelves || []; SourceShelf { required property var modelData; width: column.width; section: modelData } }
        AlbumCarousel {
            width: parent.width
            title: I18n.tr("From same artist", I18n.rev)
            model: sameArtist
        }
        AlbumCarousel {
            width: parent.width
            title: I18n.tr("Similar albums", I18n.rev)
            model: related
        }
    }

    AlbumMenu { id: albumMenu }
    ArtworkModal { id: artwork }
    PlaylistSelectionBar {
        visible: page.selectionMode && tracks.selectedCount > 0
        z: 2
        model: tracks
        playlistName: detail.title
        canRemove: false
        x: (page.width - width) / 2
        y: page.contentY + page.height - height - Metrics.selectionBottom
        onFinished: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }
}
