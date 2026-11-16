import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Patterns"
import "../Components"
import "../Primitives"

TitledFlow {
    title: "Offline music"
    OfflineSummary { width: Metrics.settingsMaxWidth; doc: ({files: 47, size: "1.6 GiB", limit: "5120", limitSize: "5.0 GiB", usage: 0.32, root: {path: "file:///music/offline/"}, limits: {values: ["2048", "5120", "unlimited"], options: ["2 GiB", "5 GiB", "Unlimited"]}}) }
    OfflineTracks { width: Metrics.settingsMaxWidth; height: Metrics.panesHeight
        doc: ({files: 2, inScope: 2, shown: 2, matching: 2, servers: [], artists: [{name: "Artist", albums: 1, tracks: 2}], rows: [
            {kind: "album", title: "Record", artist: "Artist", tracks: 2, size: "61 MiB", state: "complete", ids: [1, 2]},
            {kind: "track", id: 1, title: "First", artist: "Artist", number: 1, duration: "4:39", state: "complete"},
            {kind: "track", id: 2, title: "Second", artist: "Artist", number: 2, duration: "4:00", state: "failed", error: "The server answered 500"}]}) }
    OfflineFolders { width: Metrics.settingsMaxWidth; height: Metrics.panesHeight
        doc: ({servers: [{id: 7, name: "Music server", icon: "navidrome", files: 2, size: "61 MiB", folder: "", inherited: "file:///music/offline/Navidrome/"}]}) }
    OfflineRow { width: Metrics.settingsMaxWidth; row: ({kind: "track", id: 1, title: "Track", artist: "Artist", number: 1, duration: "3:07", state: "complete"}) }
    OfflineRootFolder { width: Metrics.confirmWidth }
    // A track's mark in each state, the arc alone, a selection's line.
    Repeater {
        model: ["queued", "downloading", "verifying", "ready", "failed"]
        OfflineMark { required property string modelData; clock: Pulse; phase: modelData; fraction: modelData === "downloading" ? 0.6 : -1 }
    }
    OfflineArc { clock: Pulse; width: Metrics.offlineRing; height: Metrics.offlineRing }
    OfflineLine { width: Metrics.confirmWidth; kind: "album"; selected: "[1]" }
    OfflineActivity { listening: false }
}
