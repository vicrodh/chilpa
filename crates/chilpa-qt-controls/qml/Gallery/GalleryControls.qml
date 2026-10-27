import QtQuick
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

TitledFlow {
    title: "Controls"
    SearchModel { id: previewSearch }
    SearchMenu { search: previewSearch }
    SearchSelection { search: previewSearch }
    SearchNavigation { search: previewSearch }
    SearchFilters { width: Metrics.confirmWidth; search: previewSearch }
    SettingsResults { width: Metrics.confirmWidth; height: Metrics.rowCompact * 5; query: "output" } // derived: a short settings preview
    SearchPreviewSwitch { search: previewSearch }
    SearchResultCard { row: ({kind: 3, title: "Record", detail: "Album · Artist", art: ""}) }
    SearchRail { width: Metrics.confirmWidth; title: "Albums"; rows: [{at: 0, kind: 3, title: "Record", detail: "Album · Artist", art: ""}]; card: Component { SearchResultCard { required property var modelData; row: modelData } } }
    QueueAbout { width: Metrics.listenArt; height: Metrics.listenArt }
    ListenNow { width: Metrics.listenArt; height: Metrics.listenArt * 2 } // derived: the art and room for a tab under it
    SearchTabs { width: Metrics.confirmWidth; tabs: [{id: "all", text: "All"}, {id: "songs", text: "Tracks"}]; tab: "songs"; selectable: true; selecting: true }
    SearchSongs { width: Metrics.confirmWidth; rows: previewChosen.rows; chosen: previewChosen }
    SearchChosenSongs { id: previewChosen; search: previewSearch; selecting: true; picked: ({"track:1": true}); rows: [{at: 0, kind: 4, title: "Song", detail: "Artist — Record", art: "", duration: "3:00", sourceIcon: "", sourceName: "", key: "track:1"}] }
    SearchRecordCard { row: ({at: 0, kind: 3, title: "Record", artist: "Artist", year: "1999", art: "", albumId: 0, sourceIcon: "", sourceName: ""}) }
    Keycap { text: "Ctrl + ▶" }
    Keycap { text: "Press keys…"; button: true; recording: true }
    ShortcutRow { width: Metrics.confirmWidth; entry: ({ id: "playback.next", label: "Next Track", shortcut: "Ctrl + ▶", modified: false }) }
    ShortcutRow { width: Metrics.confirmWidth; editing: true; recording: true; tried: "Q"; takenBy: "Queue"; entry: ({ id: "playback.mute", label: "Mute / Unmute", shortcut: "M", modified: true }) }
    ShortcutColumns { width: Metrics.settingsMaxWidth; keys: ({ col1: [{ label: "Playback", rows: [{ id: "playback.toggle", label: "Play / Pause", shortcut: "Space", modified: false }] }] }) }
    LoopRow { width: Metrics.playbackOptionsWidth }
    LoopButton {}
    QueueFooter { width: Metrics.rightPanel }
    QueueEnds { summary: "12 songs 1 hr 5 min"; remaining: 65 * 60000 } // derived: an hour and five minutes
    ShortcutsSheet {}
    CommandHelp { width: Metrics.settingsMaxWidth; room: Metrics.settingsMaxWidth; lines: [{usage:":help",description:"Command help"}] }
    ShortcutsEditor {}
    SleepTimer {}
    QuitQuestion {}
    WindowControls { id: galleryControls; window: Window.window; outputDetailsHost: false }
    GalleryAudioOutput {}
    WindowActions { controls: galleryControls; commandBar: null; sheet: null; settings: null }
    TrackFocus { window: Window.window }
    NavigationPointer { window: Window.window; enabled: false }
    CommandBar { width: Metrics.settingsMaxWidth; opened: true; Component.onCompleted: input.text = "search " }
    // The cursor helpers use a short, silent list; the buttons show their
    // state without asking the player to open a file.
    Item {
        id: cursorSample
        width: Metrics.confirmWidth
        height: Metrics.rowCompact * 5 // derived: three rows and room for their controls
        property bool rich: false
        property var viewport: null
        property var model: ({ kind: "fixture", albumId: 0, key: "", isTrack: row => row >= 0 && row < 3, select: row => {} })
        function positionAt(row, mode) {}
        ViCursor { ratings: Ratings; id: sampleCursor; host: cursorSample; view: sampleRows; menu: null }
        TrackListViewport { id: sampleViewport; model: cursorSample.model; view: sampleRows }
        Column {
            id: sampleRows
            property int count: 3
            property real contentY: 0
            function indexAt(x, y) { return 0 }
            function itemAtIndex(row) { return samples.itemAt(row) }
            function forceLayout() {}
            function positionViewAtIndex(row, mode) {}
            function positionViewAtEnd() {}
            function returnToBounds() {}
            width: parent.width
            Repeater {
                id: samples
                model: 3
                Rectangle {
                    required property int index
                    width: sampleRows.width
                    height: Metrics.rowCompact
                    color: sampleCursor.row === index ? Theme.color.fillField : "transparent"
                    Text { anchors.centerIn: parent; text: "Track " + (index + 1); color: Theme.color.textPrimary }
                }
            }
            Row {
                PillButton { text: "j"; onClicked: sampleCursor.run("down") }
                PillButton { text: "k"; onClicked: sampleCursor.run("up") }
                PillButton { text: "Keep viewport"; onClicked: sampleViewport.keep() }
            }
        }
    }
    Item {
        id: searchSample
        width: Metrics.confirmWidth
        height: Metrics.fieldHeight
        property bool compact: false
        property string text: ""
        property string source: "all"
        property bool ownInput: false
        property int result: 0
        signal opened()
        signal closed()
        function open() { opened() }
        function step(by) { result += by }
        SearchCommands { host: searchSample; search: searchSample; field: sampleQuery; flyout: searchSample; active: false }
        TextInput { id: sampleQuery; anchors.fill: parent; text: "search:source query"; color: Theme.color.textSecondary }
    }
    // The player's plan over a small window, in the layout chosen: docked,
    // the content's plane stops a gap from the bar.
    Item {
        id: sampleWindow
        width: Metrics.settingsMaxWidth / 2 // derived: half a settings page
        height: Metrics.playerHeight * 4 // derived: room for a plane over the bar
        Rectangle { anchors.fill: parent; color: Theme.color.surfaceWindow }
        Rectangle { id: sampleContent; x: Metrics.spaceWindow; y: samplePlan.planeTop; width: parent.width - x * 2; height: parent.height - y - samplePlan.contentBottom; color: Theme.color.surfaceSidebar } // derived: the window's gap on each side
        Rectangle { x: samplePlan.barX; y: samplePlan.barY; width: samplePlan.barWidth; height: samplePlan.tall; radius: Metrics.radiusPlayer; color: Theme.color.surfacePlayer }
        PlayerPlan { id: samplePlan; window: sampleWindow; content: sampleContent; shapesWindow: false }
    }
}
