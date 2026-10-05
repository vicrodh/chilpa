import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// Folders, two ways. As a tree: the library's folders on the left, and on
// the right the one that is open, with the folders inside it and its own
// tracks. Flat: every record, by the folder it is in. A folder outside the
// library, or a CD, opens from here without joining it.
Item {
    id: page

    property bool flat: Navigation.routeKey === "" && SettingsModel.kept("ui.folders.mode") === "flat"
    property real rail: Number(SettingsModel.kept("ui.folders.rail")) || Metrics.folderTreeWidth
    // The rail's width when its edge was taken.
    property real taken: 0

    FolderModel {
        id: current
        objectName: "folderContents"
        path: Navigation.routeKey
        content: true
        contentRev: Library.contentRev
        onFolderOpened: {
            page.flat = false
            SettingsModel.keep("ui.folders.mode", "tree")
        }
    }
    TrackListModel {
        id: tracks
        kind: "folder"
        filterRev: current.rev
        playingPath: Player.path
    }
    AlbumGridModel {
        id: inside
        kind: "folder"
        contentRev: current.rev
    }
    AlbumGridModel {
        id: records
        kind: page.flat ? "all" : ""
        order: Library.albumOrder
        filter: find.text
        contentRev: Library.contentRev
        columns: grid.columns
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Folders", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8

            SearchField {
                id: find
                anchors.verticalCenter: parent.verticalCenter
                visible: page.flat
                // What the title and the other tools leave, and no less than it takes to type in.
                width: Math.max(Metrics.folderTreeMin, Math.min(Metrics.screenSearch, head.width - title.width - sort.width - open.width - disc.width - modes.width - Metrics.folderTreeMin))
                placeholder: I18n.tr("Search", I18n.rev)
                onEscaped: text = ""
            }
            SortButton {
                id: sort
                anchors.verticalCenter: parent.verticalCenter
                visible: page.flat
                value: Library.albumOrder
                options: [
                    { value: "artist", text: I18n.tr("Artist", I18n.rev) },
                    { value: "title", text: I18n.tr("Title", I18n.rev) },
                    { value: "year", text: I18n.tr("Year", I18n.rev) },
                    { value: "added", text: I18n.tr("Date added", I18n.rev) }
                ]
                onPicked: (value) => Library.albumOrder = value
            }
            PillButton {
                id: open
                glyph: Icons.folderPlus
                text: I18n.tr("Open a folder…", I18n.rev)
                onClicked: current.pick()
            }
            PillButton {
                id: disc
                objectName: "openSacd"
                glyph: Icons.disc
                text: I18n.tr("Open SACD…", I18n.rev)
                onClicked: current.pickDisc()
            }
            IconButton {
                anchors.verticalCenter: parent.verticalCenter
                visible: current.hasCd
                tip: I18n.tr("Play the CD", I18n.rev)
                glyph: Icons.disc
                round: true
                onClicked: current.playCd()
            }
            SegmentedControl {
                id: modes
                anchors.verticalCenter: parent.verticalCenter
                options: [
                    { glyph: Icons.disc, tip: I18n.tr("Flat", I18n.rev) },
                    { glyph: Icons.folderOpen, tip: I18n.tr("Tree", I18n.rev) }
                ]
                current: page.flat ? 0 : 1
                onActivated: (index) => {
                    page.flat = index === 0
                    SettingsModel.keep("ui.folders.mode", page.flat ? "flat" : "tree")
                }
            }
        }
    }

    Item {
        id: body
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width

        AlbumGrid {
            id: grid
            visible: page.flat
            anchors.fill: parent
            bottomInset: Metrics.contentBottom
            model: records
            onOpen: (albumId) => Navigation.go("album", albumId)
        }

        FolderTree {
            id: tree
            visible: !page.flat
            width: Math.max(Metrics.folderTreeMin, Math.min(page.rail, body.width * Metrics.folderTreeShare)) // derived: the rail takes half at most
            height: parent.height - Metrics.contentBottom
            activePath: current.path
            menu: actions
        }
        ResizeHandle {
            visible: !page.flat
            anchors.left: tree.right
            height: tree.height
            onStarted: page.taken = tree.width
            onMoved: (delta) => page.rail = Math.max(Metrics.folderTreeMin, Math.min(body.width * Metrics.folderTreeShare, page.taken + delta))
            onReleased: SettingsModel.keep("ui.folders.rail", String(Math.round(page.rail)))
        }
        FolderDetail {
            visible: !page.flat
            anchors.left: tree.right
            anchors.leftMargin: Metrics.gridGap
            anchors.right: parent.right
            height: parent.height
            folder: current
            tracks: tracks
            albums: inside
            menu: actions
        }
    }

    FolderMenu {
        id: actions
        folder: current
    }
}
