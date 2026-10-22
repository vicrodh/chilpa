import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import app.chilpa.playlists
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "../Components"

// Searchable grid/list manager for playlist folders and playlists from every
// source. The virtual root owns every folder and every unfiled playlist.
Item {
    id: page

    property bool grid: true
    property int folderId: 0
    property string folderName: ""
    property var rows: []
    // Whose playlists it lists: every source's, "local" for the library's
    // own, or one plugin's. Kept between visits, as the Artists screen keeps
    // its own choice.
    property string source: SettingsModel.kept("ui.playlists.source") || ""
    property var sources: []
    readonly property var panels: JSON.parse(Sources.snapshot || "{}").panels || []
    function sourceLabel(id) { return id === "local" ? "Chilpa" : (panels.find(panel => panel.id === id) || {}).name || id }
    onSourceChanged: reload()
    readonly property int folderCount: rows.filter(row => row.rowKind === 1).length
    readonly property int playlistCount: rows.length - folderCount
    PlaylistPicker { id: createPicker }

    function artworkUrl(value) {
        return !value || value.indexOf("://") >= 0 ? (value || "") : "file://" + value
    }
    function reload() {
        sources = JSON.parse(PlaylistManager.sourcesJson() || "[]")
        // A source no longer there lists every one, not nothing.
        const from = sources.indexOf(source) >= 0 ? source : ""
        const raw = JSON.parse(PlaylistManager.json(search.text, filterButton.value, from,
                                                     sort.base, sort.value === sort.base, folderId))
        rows = raw.map(entry => ({rowKind: entry.kind === "Folder" ? 1 : 0,
            entryId: entry.id, name: entry.name, detail: entry.detail,
            source: entry.source, sourceName: entry.source_name,
            artwork: artworkUrl(entry.artwork_path), cover1: artworkUrl(entry.covers[0]),
            cover2: artworkUrl(entry.covers[1]), cover3: artworkUrl(entry.covers[2]),
            cover4: artworkUrl(entry.covers[3]), colorName: entry.color,
            tracks: entry.track_count, plays: entry.play_count,
            folderId: entry.folder_id || 0, local: entry.local,
            visibleInSidebar: entry.visible_sidebar}))
    }
    function enterFolder(id, name) { folderId = id; folderName = name; reload() }
    function leaveFolder() { folderId = 0; folderName = ""; reload() }
    function openPlaylist(id) {
        const window = page.Window.window
        if (typeof window?.deferNavigation === "function") window.deferNavigation(Playlists.open, id)
        else Playlists.open(id)
    }
    Component.onCompleted: reload()
    Connections { target: Playlists; function onRevChanged() { page.reload() } }

    QtObject {
        id: playlistManager
        function open(index) { const row = page.rows[index]; if (row.rowKind === 1) page.enterFolder(row.entryId, row.name); else page.openPlaylist(row.entryId) }
        function createFolder(name, color) { Playlists.createFolder(name, color) }
        function updateFolder(id, name, color) { Playlists.updateFolder(id, name, color) }
        function removeFolder(id) { Playlists.removeFolder(id) }
        function moveToFolder(id, folder) { Playlists.moveToFolder(id, folder) }
        function setVisible(id, visible) { Playlists.setVisible(id, visible) }
        function folderIds() { return JSON.parse(Playlists.catalogJson("")).folders.map(folder => folder.id) }
        function folderNames() { return JSON.parse(Playlists.catalogJson("")).folders.map(folder => folder.name) }
        function moveCustom(index, by) {
            const at = page.rows[index]
            if (at.rowKind === 1) return
            const ids = page.rows.filter(row => row.rowKind === 0).map(row => row.entryId)
            const from = ids.indexOf(at.entryId), to = from + by
            if (to < 0 || to >= ids.length) return
            const moved = ids.splice(from, 1)[0]; ids.splice(to, 0, moved)
            Playlists.setCustomOrder(JSON.stringify(ids))
        }
    }

    Column {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: parent.width - Metrics.contentHorizontalMargins
        spacing: Metrics.gap12

        Row {
            width: parent.width
            spacing: Metrics.gap8

            PillButton {
                visible: page.folderId > 0
                glyph: Icons.chevronLeft
                text: I18n.tr("Playlists", I18n.rev)
                onClicked: page.leaveFolder()
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - tools.width - (page.folderId > 0 ? Metrics.playlistManagerBack : 0)
                text: page.folderId > 0 ? page.folderName : I18n.tr("Playlist manager", I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.title
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Row {
                id: tools
                spacing: Metrics.gap6
                PillButton {
                    glyph: Icons.plus
                    text: I18n.tr("New playlist", I18n.rev)
                    onClicked: createPicker.beginCreate()
                }
                PillButton {
                    glyph: Icons.folderPlus
                    text: I18n.tr("New folder", I18n.rev)
                    onClicked: folderDialog.show(0, "", "accent")
                }
            }
        }

        Row {
            width: parent.width
            spacing: Metrics.gap8

            TextField {
                id: search
                width: Metrics.playlistManagerSearch
                placeholder: I18n.tr("Search playlists and folders", I18n.rev)
                onTextChanged: page.reload()
            }
            PillButton {
                id: filterButton
                property string value: "all"
                glyph: Icons.filter
                text: value === "folders" ? I18n.tr("Folders", I18n.rev)
                    : value === "playlists" ? I18n.tr("Playlists", I18n.rev)
                    : value === "empty" ? I18n.tr("Empty folders", I18n.rev)
                    : I18n.tr("All", I18n.rev)
                onClicked: filters.open()
                Menu {
                    id: filters
                    y: parent.height + Metrics.gap4
                    entries: [
                        {text: I18n.tr("All", I18n.rev), value: "all"},
                        {text: I18n.tr("Folders", I18n.rev), value: "folders"},
                        {text: I18n.tr("Playlists", I18n.rev), value: "playlists"},
                        {text: I18n.tr("Empty folders", I18n.rev), value: "empty"}
                    ]
                    onChosen: (index) => filterButton.value = entries[index].value
                    onClosed: page.reload()
                }
            }
            PillButton {
                id: sourceButton
                objectName: "playlistSourceFilter"
                visible: page.sources.length > 1
                glyph: Icons.server
                text: page.sources.indexOf(page.source) >= 0 ? page.sourceLabel(page.source) : I18n.tr("All sources", I18n.rev)
                onClicked: sourceMenu.open()
                Menu {
                    id: sourceMenu
                    objectName: "playlistSourceMenu"
                    y: parent.height + Metrics.gap4
                    entries: [{text: I18n.tr("All sources", I18n.rev), value: ""}].concat(page.sources.map(id => ({text: page.sourceLabel(id), value: id})))
                    onChosen: (index) => {
                        page.source = entries[index].value
                        SettingsModel.keep("ui.playlists.source", page.source)
                    }
                }
            }
            SortButton {
                id: sort
                value: "name"
                options: [
                    {value: "name", text: I18n.tr("Name", I18n.rev)},
                    {value: "recent", text: I18n.tr("Recent", I18n.rev), descending: true},
                    {value: "tracks", text: I18n.tr("Number of tracks", I18n.rev), descending: true},
                    {value: "plays", text: I18n.tr("Play count", I18n.rev), descending: true},
                    {value: "custom", text: I18n.tr("Custom", I18n.rev), fixed: true}
                ]
                onPicked: (value) => sort.value = value
                onValueChanged: page.reload()
            }
            Item { width: Math.max(0, parent.width - x - viewButtons.width); height: 1 }
            Row {
                id: viewButtons
                spacing: Metrics.gap2
                IconButton { round: true; glyph: Icons.layoutGrid; tone: page.grid ? "accent" : "normal"; tip: I18n.tr("Grid", I18n.rev); onClicked: page.grid = true }
                IconButton { round: true; glyph: Icons.list; tone: !page.grid ? "accent" : "normal"; tip: I18n.tr("List", I18n.rev); onClicked: page.grid = false }
            }
        }

        Text {
            text: I18n.trCount("{} folder", "{} folders", page.folderCount, I18n.rev)
                + " · " + I18n.trCount("{} playlist", "{} playlists", page.playlistCount, I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
    }

    Loader {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        sourceComponent: page.grid ? gridView : listView
    }

    Component {
        id: gridView
        GridView {
            leftMargin: Metrics.contentX
            rightMargin: Metrics.contentX
            bottomMargin: Metrics.contentBottom
            cellWidth: Math.max(Metrics.playlistManagerCardWidth, width / Math.max(1, Math.floor(width / Metrics.playlistManagerCardWidth)))
            cellHeight: Metrics.playlistManagerCard + Metrics.gap12
            clip: true
            model: page.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
            delegate: PlaylistManagerRow {
                width: GridView.view.cellWidth - Metrics.gap12
                grid: true
                panels: page.panels
                manager: playlistManager
                custom: sort.base === "custom"
                onEditFolder: (id, name, color) => folderDialog.show(id, name, color)
                onEditPlaylist: (id) => playlistEdit.show(id)
            }
        }
    }

    Component {
        id: listView
        ListView {
            leftMargin: Metrics.contentX
            rightMargin: Metrics.contentX
            bottomMargin: Metrics.contentBottom
            spacing: Metrics.gap6
            clip: true
            model: page.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
            delegate: PlaylistManagerRow {
                width: ListView.view.width - Metrics.contentHorizontalMargins
                grid: false
                panels: page.panels
                manager: playlistManager
                custom: sort.base === "custom"
                onEditFolder: (id, name, color) => folderDialog.show(id, name, color)
                onEditPlaylist: (id) => playlistEdit.show(id)
            }
        }
    }

    PlaylistFolderDialog { id: folderDialog }

    PlaylistEditDialog { id: playlistEdit }
}
