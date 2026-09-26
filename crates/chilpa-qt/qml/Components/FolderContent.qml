import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The directory on the right of the tree: breadcrumbs and one content plane
// that can show its child folders and albums as a compact list or grid.
Item {
    id: content

    property FolderModel folder
    property AlbumGridModel albums
    property TrackListModel tracks
    property bool grid: true

    FolderCrumbs {
        id: crumbs
        width: toolbar.x - Metrics.gap8
        height: Metrics.breadcrumb
        path: content.folder.path
        onOpened: (path) => Navigation.openFolder(path)
    }

    Row {
        id: toolbar
        anchors.right: parent.right
        anchors.verticalCenter: crumbs.verticalCenter
        spacing: Metrics.gap8

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: content.folder.folders > 0
                ? I18n.trCount("{} folder", "{} folders", content.folder.folders, I18n.rev)
                : content.albums.albums > 0
                ? I18n.trCount("{} album", "{} albums", content.albums.albums, I18n.rev)
                : content.tracks.summary
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        SegmentedControl {
            options: [
                { glyph: Icons.list, tip: I18n.tr("List", I18n.rev) },
                { glyph: Icons.layoutGrid, tip: I18n.tr("Grid", I18n.rev) }
            ]
            current: content.grid ? 1 : 0
            onActivated: (index) => content.grid = index === 1
        }
    }

    Loader {
        anchors.top: crumbs.bottom
        anchors.topMargin: Metrics.gap8
        anchors.bottom: parent.bottom
        width: parent.width
        sourceComponent: content.grid ? gridView : listView
    }

    Component {
        id: listView

        Flickable {
            clip: true
            contentWidth: width
            contentHeight: listColumn.height + Metrics.contentBottom
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: listColumn
                width: parent.width
                spacing: Metrics.gap4

                SectionHeader { visible: content.folder.folders > 0; width: parent.width; title: I18n.tr("Folders", I18n.rev) }
                Repeater {
                    model: content.folder
                    FolderEntryRow {
                        width: listColumn.width
                        onOpen: Navigation.openFolder(path)
                    }
                }
                SectionHeader { visible: content.albums.albums > 0; width: parent.width; title: I18n.tr("Albums", I18n.rev) }
                Repeater {
                    model: content.albums.albums
                    FolderAlbumRow {
                        required property int index
                        width: listColumn.width
                        albumId: content.albums.albumId(index, content.albums.rev)
                        title: content.albums.title(index, content.albums.rev)
                        artist: content.albums.subtitle(index, content.albums.rev)
                        art: content.albums.art(index, content.albums.rev)
                        onOpen: Navigation.go("album", albumId)
                    }
                }
                SectionHeader {
                    visible: content.albums.albums === 0 && content.tracks.count > 0
                    width: parent.width
                    title: I18n.tr("Tracks", I18n.rev)
                }
                TrackList {
                    visible: content.albums.albums === 0 && content.tracks.count > 0
                    width: parent.width
                    height: visible ? implicitHeight : 0
                    fitContent: true
                    showArt: false
                    model: content.tracks
                }
                Text {
                    visible: !content.folder.loading && content.folder.folders === 0 && content.tracks.count === 0
                    topPadding: Metrics.section
                    text: I18n.tr("This folder is empty.", I18n.rev)
                    font.pointSize: Type.body
                    color: Theme.color.textSecondary
                }
            }
        }
    }

    Component {
        id: gridView

        Flickable {
            clip: true
            contentWidth: width
            contentHeight: gridColumn.height + Metrics.contentBottom
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: gridColumn
                width: parent.width
                spacing: Metrics.gap12

                SectionHeader { visible: content.folder.folders > 0; width: parent.width; title: I18n.tr("Folders", I18n.rev) }
                Flow {
                    id: folderGrid
                    readonly property int columns: Math.max(1, Math.floor((width + Metrics.gridGap) / Metrics.folderTileMinWidth))
                    readonly property real tileWidth: (width - (columns - 1) * Metrics.gridGap) / columns
                    readonly property int rows: Math.ceil(content.folder.folders / columns)
                    visible: content.folder.folders > 0
                    width: parent.width
                    height: visible ? rows * Metrics.folderTileHeight + Math.max(0, rows - 1) * Metrics.gridGap : 0 // derived: every tile row and its gaps
                    spacing: Metrics.gridGap

                    Repeater {
                        model: content.folder
                        FolderTile {
                            width: folderGrid.tileWidth
                            onOpen: Navigation.openFolder(path)
                        }
                    }
                }
                SectionHeader { visible: content.albums.albums > 0; width: parent.width; title: I18n.tr("Albums", I18n.rev) }
                AlbumGrid {
                    visible: content.albums.albums > 0
                    width: parent.width
                    height: visible ? implicitHeight : 0
                    fitContent: true
                    model: content.albums
                    onOpen: (albumId) => Navigation.go("album", albumId)
                }
                TrackList {
                    visible: content.albums.albums === 0 && content.tracks.count > 0
                    width: parent.width
                    height: visible ? implicitHeight : 0
                    fitContent: true
                    showArt: true
                    rich: true
                    model: content.tracks
                }
                Text {
                    visible: !content.folder.loading && content.folder.folders === 0 && content.tracks.count === 0
                    text: I18n.tr("This folder is empty.", I18n.rev)
                    font.pointSize: Type.body
                    color: Theme.color.textSecondary
                }
            }
        }
    }
}
