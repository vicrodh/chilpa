import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"

GallerySection {
    id: section
    title: "Patterns"

    TrackListModel { id: playlistTracks; kind: "mostPlayed"; limit: 3 }
    PlaylistPickerDialog { id: pickerDialog }
    PlaylistEditDialog { id: playlistEditor }
    PlaylistFolderDialog { id: folderEditor }
    ServerSetupDialog { id: serverSetup }
    CastPicker { id: castPreview; preview: true }

    Sidebar { width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    SidebarPlaylists { width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    RightPanel { width: Metrics.rightPanel; height: Metrics.queueMin * 3 } // derived: room for both and the divider
    Row {
        spacing: Metrics.gridGap
        QueuePanel { width: Metrics.rightPanel; height: Metrics.queueMin }
        SourcesPopover {}
        LyricsPanel { width: Metrics.rightPanel; height: Metrics.queueMin }
    }
    Column {
        width: section.width - Metrics.sidebarWidth - Metrics.gridGap
        spacing: Metrics.gap12
        TabBar { width: parent.width }
        PlayerBar { width: Math.min(parent.width, Metrics.playerMaxWidth) }
        PlayerLcd { width: parent.width / 2 } // derived: half the section
        QualityTooltip {}
        SearchModel { id: sample; text: "mora" }
        SearchFlyout { search: sample }
        SearchBox { width: Metrics.screenSearch; active: false }
        PlaybackOptions {}
        SelectionBar {}
        PlaylistSelectionBar { width: implicitWidth; model: playlistTracks; playlistName: "Gallery playlist" }
        Row {
            spacing: Metrics.gap8
            PillButton { text: "New playlist modal"; onClicked: pickerDialog.beginCreate() }
            PillButton { text: "Folder modal"; onClicked: folderEditor.show(0, "", "accent") }
            PillButton { text: "Edit playlist modal"; enabled: false; onClicked: playlistEditor.show(0) }
            PillButton { text: I18n.tr("Play on", I18n.rev); onClicked: castPreview.open() }
            PillButton { text: "Add server modal"; onClicked: serverSetup.open() }
        }
        Toast { visible: true; message: "Playback failed" }
    }
}
