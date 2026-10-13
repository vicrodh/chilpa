import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/stats/qml/Patterns" as AnalysisUi

TitledFlow {
    id: section
    title: "Patterns"

    TrackListModel { id: playlistTracks; kind: "mostPlayed"; limit: 3 }
    PlaylistEditDialog { id: playlistEditor }
    PlaylistFolderDialog { id: folderEditor }
    SidebarPlaylistActions { id: playlistActions; sidebar: sampleSidebar }
    ServerSetupDialog { id: serverSetup }
    AnalysisUi.AnalysisInventoryDialog { id: inventoryPreview }
    CastPicker { id: castPreview; preview: true }
    StorageSettingsActions { current: false; visible: false }

    SystemControls {}
    NavMenu {}
    TabStrip { room: Metrics.sidebarWidth }
    Sidebar { width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    SidebarPlaylists { id: sampleSidebar; width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    RightPanel { width: Metrics.rightPanel; height: Metrics.queueMin * 3 } // derived: room for both and the divider
    Row {
        spacing: Metrics.gridGap
        QueuePanel { width: Metrics.rightPanel; height: Metrics.queueMin }
        LyricsPanel { width: Metrics.rightPanel; height: Metrics.queueMin }
        LyricsOptions {}
    }
    Column {
        width: section.width - Metrics.sidebarWidth - Metrics.gridGap
        spacing: Metrics.gap12
        TabBar { width: parent.width }
        PlayerBar { width: parent.width }
        ListenBuffers { width: parent.width; document: ({ selected: "1", tabs: [{id: "1", name: "Jazz", count: 12, playing: true}, {id: "2", name: "Later", count: 8, playing: false}] }) }
        ListenNow { width: Metrics.listenArt }
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
            PillButton { text: "Sidebar playlist menu"; onClicked: playlistActions.head(0, height) }
            PillButton { text: "Folder modal"; onClicked: folderEditor.show(0, "", "accent") }
            PillButton { text: "Edit playlist modal"; enabled: false; onClicked: playlistEditor.show(0) }
            PillButton { text: I18n.tr("Play on", I18n.rev); onClicked: castPreview.show(this) }
            PillButton { text: "Add server modal"; onClicked: serverSetup.open() }
            PillButton { text: "Analysis inventory"; onClicked: inventoryPreview.show() }
        }
        Toast { visible: true; message: "Playback failed" }
        PageTransition {
            width: Metrics.sidebarWidth
            height: Metrics.settingsRow
            sourceComponent: Component {
                Rectangle { color: Theme.color.surfacePanel; radius: Metrics.radiusControl }
            }
        }
    }
}
