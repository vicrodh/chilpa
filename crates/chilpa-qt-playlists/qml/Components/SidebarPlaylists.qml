import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import app.chilpa.playlists

// The sidebar's playlists (design/02 2, item 6): the label with a button to
// make one, a filter, and the list, which scrolls in whatever height is
// left. A right click offers the playlist's menu; a double click renames it.
Item {
    id: section; objectName: "sidebarPlaylists"

    property string order: SettingsModel.kept("ui.playlists.order") || "name"
    property bool ascending: SettingsModel.kept("ui.playlists.ascending") !== "false"
    // A new order comes its own way round.
    function setOrder(value) { order = value; ascending = true; SettingsModel.keep("ui.playlists.order", value); SettingsModel.keep("ui.playlists.ascending", "true"); reload() }
    function reverseOrder() { ascending = !ascending; SettingsModel.keep("ui.playlists.ascending", String(ascending)); reload() }
    property var rows: []
    PlaylistTree { id: playlistTree }
    KeyedModel { id: stableRows; items: section.rows }
    property var expanded: ({})
    function reload() {
        const catalog = JSON.parse(Playlists.catalogJson(filter.text))
        rows = playlistTree.rows(catalog, order, ascending, filter.text, expanded, I18n.tr("Playlists", I18n.rev))
    }
    function toggleFolder(id) { expanded[id] = !expanded[id]; reload() }
    function openPlaylist(id) {
        const window = section.Window.window
        if (typeof window?.deferNavigation === "function") window.deferNavigation(Playlists.open, id)
        else Playlists.open(id)
    }
    Component.onCompleted: reload()
    Connections { target: Library; function onRemoteRevChanged() { section.reload() } function onRevChanged() { section.reload() } }
    Connections { target: Playlists; function onRevChanged() { section.reload() } }

    Item {
        id: head
        width: parent.width
        height: Metrics.button24

        Text {
            x: Metrics.navItemPadding
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Playlists", I18n.rev)
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textTertiary
        }
        IconButton {
            id: addButton
            anchors.right: parent.right
            size: Metrics.button24
            glyph: Icons.plus
            tip: I18n.tr("New playlist", I18n.rev)
            onClicked: AddToPlaylist.show("[]")
        }
        IconButton {
            anchors.right: addButton.left
            size: Metrics.button24
            glyph: Icons.dots
            tip: I18n.tr("Playlist options", I18n.rev)
            onClicked: actions.head(x, head.height)
        }
    }

    TextField {
        id: filter
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap6
        width: parent.width
        height: Metrics.playlistFilter
        placeholder: I18n.tr("Filter playlists", I18n.rev)
        onTextChanged: section.reload()
        Keys.onEscapePressed: text = ""
    }

    ListView {
        id: view
        objectName: "sidebarPlaylistList"
        anchors.top: filter.bottom
        anchors.topMargin: Metrics.gap6
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: stableRows
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}

        delegate: Loader {
            id: host
            required property int index
            required property var entry

            x: entry.depth * Metrics.gap8 // derived: one small inset per folder depth
            width: ListView.view.width - x
            height: Metrics.playlistRowHeight
            sourceComponent: entry.rowKind === 2 ? groupDelegate : entry.rowKind === 1 ? folderDelegate : playlistDelegate
            Component { id: groupDelegate; PlaylistGroupHeader { title:host.entry.name; sourceName:host.entry.sourceName; sourceIcon:host.entry.sourceIcon } }

            Component {
                id: folderDelegate
                PlaylistFolderRow {
                    width: host.width
                    index: host.index
                    folderId: host.entry.folderId
                    name: host.entry.name
                    tracks: host.entry.tracks
                    folderColor: host.entry.folderColor
                    sourceIcon: host.entry.sourceIcon
                    expanded: host.entry.expanded
                    onToggled: section.toggleFolder(host.entry.expandKey || folderId)
                    onMenu: (left, top) => {
                        if (host.entry.folderId < 0 && !host.entry.nativeRef) return
                        const at = mapToItem(section, left, top)
                        actions.row(host.entry, at.x, at.y)
                    }
                }
            }
            Component {
                id: playlistDelegate
                PlaylistRow {
                    id: row
                    width: host.width
                    index: host.index
                    playlistId: host.entry.playlistId
                    name: host.entry.name
                    tracks: host.entry.tracks
                    source: host.entry.source
                    sourceName: host.entry.sourceName
                    sourceIcon: host.entry.sourceIcon
                    showSourceBadge: host.entry.showSourceBadge
                    local: host.entry.local
                    artwork: host.entry.artwork
                    folderId: host.entry.folderId
                    visibleInSidebar: host.entry.visibleInSidebar
                    covers: host.entry.covers
                    active: Navigation.screen === (playlistId < 0 ? "album" : "playlist") && Navigation.routeId === playlistId
                    onOpened: section.openPlaylist(playlistId)
                    onMenu: (left, top) => {
                        const at = row.mapToItem(section, left, top)
                        actions.row(host.entry, at.x, at.y)
                    }
                    onRenamed: (text) => {
                        if (local)
                            Playlists.rename(playlistId, text)
                    }
                }
            }
        }
    }

    Text {
        visible: section.rows.length === 0 && filter.text === ""
        anchors.top: filter.bottom
        anchors.topMargin: Metrics.gap10
        x: Metrics.navItemPadding
        width: parent.width - x
        wrapMode: Text.Wrap
        text: I18n.tr("No playlists yet.", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }

    SidebarPlaylistActions {
        id: actions
        sidebar: section
        onCreatePlaylist: AddToPlaylist.show("[]")
    }
}
