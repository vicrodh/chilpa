import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The sidebar's playlists (design/02 2, item 6): the label with a button to
// make one, a filter, and the list, which scrolls in whatever height is
// left. A right click offers the playlist's menu; a double click renames it.
Item {
    id: section

    PlaylistPicker { id: createPicker }

    property var rows: []
    property var expanded: ({})
    function reload() {
        const catalog = JSON.parse(Playlists.catalogJson(filter.text))
        const result = []
        for (const folder of catalog.folders) {
            const children = catalog.playlists.filter(p => p.folder_id === folder.id)
            if (filter.text !== "" || children.length > 0) {
                result.push({rowKind: 1, playlistId: 0, name: folder.name,
                    tracks: folder.playlist_count, source: "", sourceName: "",
                    local: true, artwork: "", folderId: folder.id,
                    visibleInSidebar: true, cover1: "", cover2: "", cover3: "",
                    cover4: "", folderColor: folder.color,
                    expanded: filter.text !== "" || !!expanded[folder.id]})
                if (filter.text !== "" || expanded[folder.id])
                    for (const playlist of children) result.push(playlistRow(playlist))
            }
        }
        for (const playlist of catalog.playlists)
            if (playlist.folder_id === null) result.push(playlistRow(playlist))
        rows = result
    }
    function playlistRow(p) {
        return {rowKind: 0, playlistId: p.id, name: p.name, tracks: p.track_count,
            source: p.source, sourceName: p.provider, local: p.mixed,
            artwork: p.artwork_path, folderId: p.folder_id || 0,
            visibleInSidebar: p.visible_sidebar, cover1: p.covers[0] || "",
            cover2: p.covers[1] || "", cover3: p.covers[2] || "",
            cover4: p.covers[3] || "", folderColor: "", expanded: false}
    }
    function toggleFolder(id) { expanded[id] = !expanded[id]; reload() }
    Component.onCompleted: reload()
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
            tip: I18n.tr("Manage playlists", I18n.rev)
            onClicked: Navigation.go("playlistManager", 0)
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
        ScrollBar.vertical: ScrollBar {}

        delegate: Loader {
            id: host
            required property int index
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
            required property url cover1
            required property url cover2
            required property url cover3
            required property url cover4
            required property string folderColor
            required property bool expanded

            width: ListView.view.width
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
                    expanded: host.expanded
                    onToggled: section.toggleFolder(folderId)
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
                    local: host.local
                    artwork: host.artwork
                    folderId: host.folderId
                    visibleInSidebar: host.visibleInSidebar
                    cover1: host.cover1
                    cover2: host.cover2
                    cover3: host.cover3
                    cover4: host.cover4
                    active: Navigation.screen === "playlist" && Navigation.routeId === playlistId
                    onOpened: Playlists.open(playlistId)
                    onMenu: (left, top) => {
                        if (!local) {
                            Playlists.open(playlistId)
                            return
                        }
                        const at = row.mapToItem(section, left, top)
                        actions.row = row
                        actions.show(playlistId, name, at.x, at.y)
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

    PlaylistMenu {
        id: actions
        property PlaylistRow row
        onRename: if (row) row.editing = true
    }
}
