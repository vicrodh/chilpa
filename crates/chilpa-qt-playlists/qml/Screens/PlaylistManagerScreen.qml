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
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "../Components"
import "../Patterns"

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

    // Whose a playlist is: the library's own wear Chilpa's mark, a source's its icon.
    function panelOf(source) { return panels.find(panel => panel.id === source) || ({}) }
    function originOf(row) { return row.rowKind === 1 ? ({}) : row.local ? ({chilpa: true}) : ({icon: panelOf(row.source).icon_image || panelOf(row.source).icon || ""}) }
    function ownerOf(row) { return row.local ? "Chilpa" : panelOf(row.source).name || row.sourceName }
    function reload() {
        sources = JSON.parse(PlaylistManager.sourcesJson() || "[]")
        // A source no longer there lists every one, not nothing.
        const from = sources.indexOf(source) >= 0 ? source : ""
        const raw = JSON.parse(PlaylistManager.json(bar.query, bar.kind, from, bar.order, bar.ascending, folderId))
        rows = raw.map(entry => ({rowKind: entry.kind === "Folder" ? 1 : 0,
            entryId: entry.id, name: entry.name, source: entry.source, sourceName: entry.source_name,
            artwork: entry.artwork_path || "", covers: entry.covers || [], colorName: entry.color,
            tracks: entry.track_count, local: entry.local, writable: entry.writable === true,
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
        function folderColor(name) {
            return !SettingsModel.folderColors ? Theme.color.textSecondary
                : name === "positive" ? Theme.color.statusPositive
                : name === "warning" ? Theme.color.statusWarning
                : name === "negative" ? Theme.color.statusNegative
                : Theme.color.accent
        }
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

        PlaylistManagerTools {
            id: bar
            width: parent.width
            sources: page.sources
            source: page.source
            grid: page.grid
            sourceLabel: page.sourceLabel
            onChanged: page.reload()
            onSourceChosen: (value) => {
                page.source = value
                SettingsModel.keep("ui.playlists.source", value)
            }
            onViewChosen: (grid) => page.grid = grid
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

    // The grid draws each row as the cards of the rest of the app.
    Component {
        id: gridView
        GridView {
            leftMargin: Metrics.contentX
            rightMargin: Metrics.contentX
            bottomMargin: Metrics.contentBottom
            cellWidth: CardLayout.cardWidth + Metrics.gridGap
            cellHeight: CardLayout.cardHeight + Metrics.gridGap
            clip: true
            model: page.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            delegate: PlaylistCard {
                id: card
                required property var modelData
                required property int index
                name: modelData.name
                folder: modelData.rowKind === 1
                detail: folder ? I18n.trCount("{} playlist", "{} playlists", modelData.tracks, I18n.rev)
                    : page.ownerOf(modelData) + " · " + I18n.trCount("{} track", "{} tracks", modelData.tracks, I18n.rev)
                art: modelData.artwork
                covers: modelData.covers
                origin: page.originOf(modelData)
                folderColor: playlistManager.folderColor(modelData.colorName)
                onActivated: playlistManager.open(index)
                onPlayed: Playlists.play(modelData.entryId, false)
                onMenuAsked: (x, y) => rowMenu.show(Object.assign({index: index}, modelData), card, x, y)
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
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            delegate: PlaylistManagerRow {
                id: line
                width: ListView.view.width - Metrics.contentHorizontalMargins
                panels: page.panels
                manager: playlistManager
                custom: bar.order === "custom"
                onMenuAsked: (x, y) => rowMenu.show(Object.assign({index: index}, page.rows[index]), line, x, y)
            }
        }
    }

    PlaylistManagerActions {
        id: rowMenu
        manager: playlistManager
        onEditFolder: (id, name, color) => folderDialog.show(id, name, color)
        onEditPlaylist: (id) => playlistEdit.show(id)
        onEditSource: (id) => sourceEditor.begin(JSON.parse(Sources.card(id, Library.remoteRev) || "{}"), "update")
    }
    SourcePlaylistEditor { id: sourceEditor; objectName: "sourcePlaylistEditor" }

    PlaylistFolderDialog { id: folderDialog }

    PlaylistEditDialog { id: playlistEdit }
}
