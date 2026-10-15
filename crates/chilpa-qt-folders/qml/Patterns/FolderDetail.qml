import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"

// The folder that is open: what it is called and how much it holds, the
// folders inside it, the records inside it, and its own tracks. A folder
// that is a record is drawn as the record it is, with all a record has.
// Its play button plays the folder with everything under it.
Item {
    id: detail

    property FolderModel folder
    property TrackListModel tracks
    property AlbumGridModel albums
    property FolderMenu menu
    // The folders that are not records.
    readonly property int plain: folder.folders - (albums ? albums.albums : 0)
    property bool grid: SettingsModel.kept("ui.folders.detail") !== "list"
    // The discs of a box set side by side shown as the box, or each on its own.
    property bool collections: SettingsModel.kept("ui.folders.collections") === "true"
    Binding { target: detail.folder; property: "grouped"; value: detail.collections }
    readonly property string name: folder.path.replace(/\/+$/, "").split("/").pop()
    readonly property string sought: find.text.trim().toLowerCase()

    function context(item, path, left, top) {
        const at = item.mapToItem(detail.menu.parent, left, top)
        detail.menu.show(path, at.x, at.y)
    }

    Text {
        visible: detail.folder.path === ""
        anchors.centerIn: parent
        text: I18n.tr("Select a folder to see its contents.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }

    Item {
        id: head
        visible: detail.folder.path !== ""
        width: parent.width
        height: Metrics.button40

        IconButton {
            id: play
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Play", I18n.rev)
            glyph: Icons.playerPlayFilled
            filled: true
            round: true
            tone: "solid"
            size: Metrics.button40
            onClicked: detail.folder.playAll()
        }
        Column {
            anchors.left: play.right
            anchors.leftMargin: Metrics.gap12
            anchors.right: grouping.left
            anchors.rightMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter

            Text { width: parent.width; text: detail.name; elide: Text.ElideMiddle; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
            Text {
                width: parent.width
                text: detail.folder.inLibrary ? I18n.trCount("{} track", "{} tracks", detail.folder.tracks, I18n.rev)
                                              : I18n.tr("Not in the library: this is the folder as it is on disk.", I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }
        Row {
            id: grouping
            anchors.right: tools.left
            anchors.rightMargin: Metrics.gap16
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8
            Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Group collections", I18n.rev); font.pointSize: Type.meta; color: Theme.color.textSecondary }
            Toggle {
                anchors.verticalCenter: parent.verticalCenter
                checked: detail.collections
                onToggled: (on) => {
                    detail.collections = on
                    SettingsModel.keep("ui.folders.collections", on ? "true" : "false")
                }
            }
        }
        SegmentedControl {
            id: tools
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            options: [
                { glyph: Icons.layoutGrid, tip: I18n.tr("Grid", I18n.rev) },
                { glyph: Icons.list, tip: I18n.tr("List", I18n.rev) }
            ]
            current: detail.grid ? 0 : 1
            onActivated: (index) => {
                detail.grid = index === 0
                SettingsModel.keep("ui.folders.detail", detail.grid ? "grid" : "list")
            }
        }
    }

    Flickable {
        visible: detail.folder.path !== ""
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap16
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        contentWidth: width
        contentHeight: body.height + Metrics.contentBottom
        boundsBehavior: Flickable.StopAtBounds
        // In the middle of the page's margin to its right, off the cards.
        ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }

        Column {
            id: body
            width: parent.width - Metrics.gap10
            spacing: Metrics.gap12

            Item {
                visible: detail.plain > 0
                width: parent.width
                height: Math.max(folders.height, find.height)

                SectionHeader { id: folders; anchors.verticalCenter: parent.verticalCenter; width: parent.width - find.width - Metrics.gap12; title: I18n.tr("Folders", I18n.rev) }
                SearchField {
                    id: find
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(Metrics.screenSearch, parent.width * Metrics.folderTreeShare)
                    placeholder: I18n.tr("Search folders", I18n.rev)
                    onEscaped: text = ""
                }
            }
            Flow {
                id: cards
                readonly property int columns: Math.max(1, Math.floor((width + Metrics.gap12) / (Metrics.folderCardMin + Metrics.gap12)))
                readonly property real cardWidth: Math.floor((width - (columns - 1) * Metrics.gap12) / columns)
                visible: detail.grid && detail.plain > 0
                width: parent.width
                spacing: Metrics.gap12

                Repeater {
                    model: detail.grid ? detail.folder : null
                    FolderCard {
                        id: card
                        visible: album === 0 && (detail.sought === "" || name.toLowerCase().includes(detail.sought))
                        width: cards.cardWidth
                        onOpen: Navigation.openFolder(path)
                        onContext: (left, top) => detail.context(card, path, left, top)
                    }
                }
            }
            Column {
                visible: !detail.grid && detail.plain > 0
                width: parent.width

                Repeater {
                    model: detail.grid ? null : detail.folder
                    FolderEntryRow {
                        id: entry
                        visible: album === 0 && (detail.sought === "" || name.toLowerCase().includes(detail.sought))
                        width: body.width
                        height: visible ? Metrics.folderListRow : 0
                        onOpen: Navigation.openFolder(path)
                        onContext: (left, top) => detail.context(entry, path, left, top)
                    }
                }
            }

            SectionHeader { visible: records.visible; width: parent.width; title: I18n.tr("Albums", I18n.rev) }
            AlbumGrid {
                id: records
                visible: detail.albums && detail.albums.albums > 0
                width: parent.width
                height: visible ? implicitHeight : 0
                fitContent: true
                list: !detail.grid
                model: detail.albums
                onOpen: (albumId) => Navigation.go("album", albumId)
            }
            Binding {
                target: detail.albums
                property: "columns"
                value: records.columns
            }

            SectionHeader { visible: detail.tracks.count > 0; width: parent.width; title: I18n.tr("Tracks", I18n.rev) }
            TrackList {
                visible: detail.tracks.count > 0
                width: parent.width
                height: visible ? implicitHeight : 0
                fitContent: true
                header: false
                showAlbum: false
                showRating: width > Metrics.folderTreeWidth * 2 // derived: the title comes before the stars
                model: detail.tracks
            }
            Text {
                visible: !detail.folder.loading && detail.folder.tracks === 0
                topPadding: Metrics.section
                text: I18n.tr("This folder has no tracks.", I18n.rev)
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
        }
    }
}
