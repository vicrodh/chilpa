import QtQuick
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

// The playlists' pieces: the manager's cards, the picker's list, its "Already
// in" column and its question about repeats, and the picker itself.
Item {
    id: gallery
    implicitWidth: pieces.implicitWidth + Metrics.contentX * 2 // derived: the samples and their margins
    implicitHeight: pieces.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins

    readonly property var ours: ({ playlist: { id: 1, name: "Evening music", track_count: 12 }, icon: "chilpa", accepts: true, same_source: true, exact: 2, similar: 0 })
    readonly property var theirs: ({ playlist: { id: -4, name: "Road trip", track_count: 30 }, icon: "", accepts: true, same_source: false, exact: 0, similar: 0 })

    QtObject {
        id: playlistManager
        function open(index) {}
        function moveCustom(index, by) {}
        function folderIds() { return [] }
        function folderNames() { return [] }
        function setVisible(id, visible) {}
        function moveToFolder(id, folder) {}
        function removeFolder(id) {}
    }
    PlaylistPickerModal {}
    PlaylistTree { id: playlistTree }

    Column {
        id: pieces
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.gap16
        Text {
            text: "Playlist tree rows: " + playlistTree.rows({playlists:[gallery.ours.playlist, gallery.theirs.playlist]}, "name", true, "", {}).length
            font.pointSize: Type.body; color: Theme.color.textPrimary
        }
        Row {
            spacing: Metrics.gap8
            PillButton { text: "Add to playlist modal"; onClicked: AddToPlaylist.show("[1,2,3]") }
            PillButton { text: "New playlist modal"; onClicked: AddToPlaylist.show("[]") }
        }
        Row {
            spacing: Metrics.gap12
            PlaylistSortButton { playlistId: 1 }
            PlaylistManagerRow { width: Metrics.playlistManagerCardWidth; index: 0; rowKind: 1; entryId: 1; name: "Road music"; detail: ""; source: ""; sourceName: "Local"; artwork: ""; cover1: ""; cover2: ""; cover3: ""; cover4: ""; colorName: "warning"; tracks: 4; plays: 0; folderId: 0; local: true; visibleInSidebar: true; manager: playlistManager }
            PlaylistManagerRow { width: Metrics.playlistManagerCardWidth; index: 1; rowKind: 0; entryId: 2; name: "Late Night"; detail: "Quiet records"; source: "demo"; sourceName: "Example source"; artwork: ""; cover1: ""; cover2: ""; cover3: ""; cover4: ""; colorName: ""; tracks: 18; plays: 9; folderId: 0; local: false; visibleInSidebar: true; manager: playlistManager }
        }
        Row {
            spacing: Metrics.gap16
            PlaylistChoiceList { width: Metrics.playlistPickerWidth - Metrics.playlistAlreadyIn - Metrics.modalInsets; rows: [gallery.theirs]; lastUsed: gallery.ours }
            PlaylistAlreadyIn { width: Metrics.playlistAlreadyIn; rows: [gallery.ours]; trackCount: 3 }
        }
        PlaylistHeader { width: Metrics.settingsMaxWidth; name: "Evening music"; sourceName: "Local"; meta: "12 tracks · 48 min"; description: "What plays after dinner."; markKey: "1"; canSelect: true }
        PlaylistTargetRow { width: Metrics.playlistModalWidth; target: gallery.ours }
        PlaylistRepeats { width: Metrics.playlistModalWidth; height: Metrics.playlistModalHeight / 3; repeated: 2; trackCount: 3 } // derived: a third of a modal
    }
}
