import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.sources
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/qml"

ApplicationWindow {
    id: window
    visible: true; width: 1280; height: 800
    property int sample: 0
    property int step: 0
    property double started: 0
    property bool awaiting: false
    property bool prepared: false
    readonly property bool queueBusy: QueueActivity.busy
    readonly property string source: sample === 0 ? "app.test.navigation" : "app.test.second"
    ScreenHost { id: host; anchors.fill: parent }
    TrackListModel { id: tracks; kind: "source"; key: window.source + " section:tracks"; filterRev: Library.remoteRev; playingPath: Player.path }
    function find(item, name) {
        if (item?.objectName === name) return item
        for (const child of item?.children || []) { const found = find(child, name); if (found) return found }
        return null
    }
    function fail(message) { console.log("SMOKE FAIL " + message); Qt.exit(1) }
    function request() {
        started = Date.now(); awaiting = true
        console.log("SMOKE source link requested sample=" + sample + " step=" + step)
        if (step === 0) {
            const rows = find(host.item, "trackListView")
            for (let row = rows.count - 1; row >= 0; row--) {
                if (rows.model.canGo(row, "artist")) { rows.model.go(row, "artist:0"); return }
            }
            fail("the queued source track has no artist destination")
        } else if (step === 1) tracks.go(0, "album")
        else Navigation.back()
    }
    Timer {
        interval: 100; running: true; repeat: true
        onTriggered: {
            Sources.refresh()
            ListenList.refresh()
            if (window.step !== 0 || window.awaiting || !tracks.ready || !tracks.count) return
            if (!window.prepared) {
                window.prepared = true
                SettingsModel.density = window.sample === 0 ? "rich" : "compact"
                tracks.listAction("buffer:new", -1, -1); Navigation.go("listenlist", 0); return
            }
            const rows = window.find(host.item, "trackListView")
            if (host.item?.selectedQueue && rows?.model.count >= tracks.count) window.request()
        }
    }
    Connections {
        target: window
        function onFrameSwapped() {
            if (!window.awaiting || !host.item) return
            if (window.step === 1) {
                if (Navigation.screen !== "album" || !host.item.albumId) return
                if (host.item.albumId >= 0) { window.fail("a source track opened a local album"); return }
            } else {
                const key = window.source + " native:artist:0"
                if (host.item.key !== key) return
                if (Navigation.routeKey !== key || Navigation.screen !== "source") { window.fail("artist source was lost"); return }
            }
            const elapsed = Date.now() - window.started
            console.log("SMOKE source artist sample=" + window.sample + " step=" + window.step + " first-frame=" + elapsed + "ms")
            if (elapsed > 500) { window.fail("source navigation took " + elapsed + "ms"); return }
            window.awaiting = false; settle.restart()
        }
    }
    Timer {
        id: settle; interval: 1600
        onTriggered: {
            if (++window.step < 3) { window.request(); return }
            if (++window.sample < 2) { window.step = 0; window.prepared = false; return }
            console.log("SMOKE source artist links completed"); Qt.exit(0)
        }
    }
    Timer { interval: 25000; running: true; onTriggered: window.fail("source artist links timed out at " + sample + ":" + step) }
}
