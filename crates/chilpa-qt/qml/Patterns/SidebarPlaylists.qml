import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "../Components"
import "../Components/PlaylistTree.js" as PlaylistTree

// The sidebar's playlists (design/02 2, item 6): the label with a button to
// make one, a filter, and the list, which scrolls in whatever height is
// left. A right click offers the playlist's menu; a double click renames it.
Item {
    id: section; objectName: "sidebarPlaylists"

    PlaylistPicker { id: createPicker }

    property string order: SettingsModel.kept("ui.playlists.order") || "name"
    property bool ascending: SettingsModel.kept("ui.playlists.ascending") !== "false"
    function setOrder(value) { order = value; SettingsModel.keep("ui.playlists.order", value); reload() }
    function reverseOrder() { ascending = !ascending; SettingsModel.keep("ui.playlists.ascending", String(ascending)); reload() }
    property var rows: []
    property var expanded: ({})
    function reload() {
        const catalog = JSON.parse(Playlists.catalogJson(filter.text))
        rows = PlaylistTree.rows(catalog, order, ascending, filter.text, expanded)
    }
    function toggleFolder(id) { expanded[id] = !expanded[id]; reload() }
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
            onClicked: createPicker.beginCreate()
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
        anchors.top: filter.bottom
        anchors.topMargin: Metrics.gap6
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: section.rows
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}

        delegate: Loader {
            id: host
            required property int index
            required property int depth
            required property int rowKind
            required property int playlistId
            required property string name
            required property int tracks
            required property string source
            required property string sourceName
            required property bool local
            required property string artwork
            required property int folderId
            required property bool visibleInSidebar
            required property var covers
            required property string folderColor
            required property bool expanded
            required property string sourceIcon
            required property bool showSourceBadge

            x: depth * Metrics.gap8 // derived: one small inset per folder depth
            width: ListView.view.width - x
            height: Metrics.playlistRowHeight
            sourceComponent: rowKind === 1 ? folderDelegate : playlistDelegate

            Component {
                id: folderDelegate
                PlaylistFolderRow {
                    width: host.width
                    index: host.index
                    folderId: host.folderId
                    name: host.name
                    tracks: host.tracks
                    folderColor: host.folderColor
                    sourceIcon: host.sourceIcon
                    expanded: host.expanded
                    onToggled: section.toggleFolder(section.rows[host.index].expandKey || folderId)
                    onMenu: (left, top) => {
                        if (host.folderId < 0 && !section.rows[host.index].nativeRef) return
                        const at = mapToItem(section, left, top)
                        actions.row(section.rows[host.index], at.x, at.y)
                    }
                }
            }
            Component {
                id: playlistDelegate
                PlaylistRow {
                    id: row
                    width: host.width
                    index: host.index
                    playlistId: host.playlistId
                    name: host.name
                    tracks: host.tracks
                    source: host.source
                    sourceName: host.sourceName
                    sourceIcon: host.sourceIcon
                    showSourceBadge: host.showSourceBadge
                    local: host.local
                    artwork: host.artwork
                    folderId: host.folderId
                    visibleInSidebar: host.visibleInSidebar
                    covers: host.covers
                    active: Navigation.screen === (playlistId < 0 ? "album" : "playlist") && Navigation.routeId === playlistId
                    onOpened: Playlists.open(playlistId)
                    onMenu: (left, top) => {
                        const at = row.mapToItem(section, left, top)
                        actions.row(section.rows[host.index], at.x, at.y)
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
        onCreatePlaylist: createPicker.beginCreate()
    }
}
