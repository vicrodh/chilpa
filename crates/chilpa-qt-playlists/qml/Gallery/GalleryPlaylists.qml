import QtQuick
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

// The playlists' pieces: the manager's cards, the picker's list, its "Already
// in" column and its question about repeats, the picker itself, and the
// importer's steps.
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
        function folderColor(name) { return Theme.color.accent }
    }
    PlaylistPickerModal {}
    PlaylistsFlyout {}
    SidebarPlaylistsHead { width: Metrics.sidebarWidth }
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
            Column {
                width: Metrics.confirmWidth
                PlaylistManagerRow { width: parent.width; index: 0; rowKind: 1; entryId: 1; name: "Road music"; artwork: ""; colorName: "warning"; tracks: 4; local: true; source: ""; sourceName: ""; unsent: false; manager: playlistManager }
                PlaylistManagerRow { width: parent.width; index: 1; rowKind: 0; entryId: 2; name: "Late Night"; artwork: ""; colorName: ""; tracks: 0; local: false; source: "demo"; sourceName: "Example source"; unsent: true; manager: playlistManager }
            }
            PlaylistManagerActions { manager: playlistManager }
            PlaylistManagerTools { width: Metrics.settingsMaxWidth; sources: ["local", "demo"] }
            PlaylistSourcesPopup { sources: ["local", "demo"]; labelOf: id => id }
        }
        Row {
            spacing: Metrics.gap16
            PlaylistChoiceList { width: Metrics.playlistPickerWidth - Metrics.playlistAlreadyIn - Metrics.modalInsets; rows: [gallery.theirs]; lastUsed: gallery.ours }
            PlaylistAlreadyIn { width: Metrics.playlistAlreadyIn; rows: [gallery.ours]; trackCount: 3 }
        }
        PlaylistHeader { width: Metrics.settingsMaxWidth; name: "Evening music"; sourceName: "Local"; meta: "12 tracks · 48 min"; description: "What plays after dinner."; markKey: "1"; canSelect: true }
        PlaylistTargetRow { width: Metrics.playlistModalWidth; target: gallery.ours }
        PlaylistRepeats { width: Metrics.playlistModalWidth; height: Metrics.playlistModalHeight / 3; repeated: 2; trackCount: 3 } // derived: a third of a modal
        ImportOriginStep {
            width: Metrics.confirmWidth; reading: true
            inputs: [{id: "url", name: "Playlist link", hint: "A public playlist link", kind: "text"}, {id: "file", name: "Playlist file", kind: "file"},
                {id: "station", name: "Stations", kind: "station", choices: [{id: 0, name: "Library"}, {id: 1, name: "Mix"}]}]
        }
        ImportDestinationRow { width: Metrics.confirmWidth; title: "Chilpa playlist"; detail: "Best quality from any connected source, kept in Chilpa"; sourceInfo: ({chilpa: true}); checked: true }
        ImportDestinationRow { width: Metrics.confirmWidth; title: "Your library"; detail: "Only songs already in your library"; sourceInfo: ({local: true}) }
        ImportDestinationsStep {
            width: Metrics.confirmWidth; height: Metrics.playlistModalHeight; selected: ["mixed", "demo"]
            status: ({phase: "preview", preview: {name: "Evening music", description: "Shared by a friend", total: 42}, settings: {}})
            sources: [{id: "demo", name: "Example source", writable: true}]
            places: ({mixed: {name: "Chilpa playlist", detail: "Best quality from any connected source, kept in Chilpa", info: {chilpa: true}},
                local: {name: "Your library", detail: "Only songs already in your library", info: {local: true}},
                demo: {name: "Example source", detail: "Playlist in your Example source account", info: {icon: ""}, writable: true}})
        }
        ImportReviewRow { width: Metrics.confirmWidth; number: 1; original: ({title: "Song", artist: "Band"}); match: ({title: "Song", confidence: 0.98}); sourceInfo: ({chilpa: true}); quality: "FLAC · 24 bit · 96 kHz" }
        ImportReviewRow { width: Metrics.confirmWidth; rowIndex: 1; number: 2; original: ({title: "Lost", artist: "Band"}) }
        ImportReviewRow { width: Metrics.confirmWidth; number: 3; original: ({title: "Late", artist: "Band"}); problem: "Example source did not answer"; problemDetail: "the plugin is not running" }
        ImportReviewStep {
            width: Metrics.confirmWidth; height: Metrics.playlistModalHeight
            places: ({mixed: {name: "Chilpa playlist", info: {chilpa: true}}, demo: {name: "Example source", info: {icon: ""}}})
            status: ({phase: "matching", targets: ["mixed"], completed: 1, total_work: 2, current: "Band — Lost", row_count: 1,
                rows: [{index: 1, original: {title: "Song", artist: "Band"}, errors: {}, matches: {mixed: {source: "demo", title: "Song", quality: {codec: "flac", bits: 16, rate: 44100}, confidence: 0.95}}}]})
        }
    }
}
