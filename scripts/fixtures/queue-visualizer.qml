import QtQuick
import app.chilpa
import app.chilpa.controls
import app.chilpa.immersive

Item {
    id: smoke
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property var mainWindow: null
    property int phase: 0
    property int mode: 0
    property int began: 0
    property int ticks: 0
    property bool capturing: false
    property int frames: 0
    property int parkedFrames: 0
    property var counts: ({bars:0, waveform:0, energy:0, goniometer:0, oscilloscope:0})
    property var names: ["bars", "waveform", "energy", "goniometer", "oscilloscope", "bars", "bars"]
    SettingsPage { id: audio; section: "audio" }
    function check(ok, message) { if (!ok) throw new Error(message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) { const found = find(child, name); if (found) return found }
        return null
    }
    function item(name) { const found = find(mainWindow.contentItem, name); check(!!found, "Missing " + name); return found }
    function selectMode(next) {
        mode = next
        item("listenNow").visualizationMode = next
        began = ticks
        frames = counts[names[next]]
    }
    Connections {
        target: Visualizer
        function onBarsChanged() { ++smoke.counts.bars }
        function onWaveformChanged() { ++smoke.counts.waveform }
        function onEnergyChanged() { ++smoke.counts.energy }
        function onGoniometerChanged() { ++smoke.counts.goniometer }
        function onOscilloscopeChanged() { ++smoke.counts.oscilloscope }
    }
    Timer {
        interval: 75
        running: true
        repeat: true
        onTriggered: {
            try {
                if (smoke.capturing) return
                ++smoke.ticks
                if (smoke.phase === 0) {
                    const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
                    check(component.status === Component.Ready, component.errorString())
                    smoke.mainWindow = component.createObject(null)
                    smoke.mainWindow.width = 1480; smoke.mainWindow.height = 950
                    smoke.phase = 1
                    return
                }
                if (smoke.phase === 1) {
                    if (!Player.path.endsWith("fixture.wav")) return
                    audio.set("audio.device", "file:capture")
                    Player.toggle()
                    Navigation.go("listenlist", 0)
                    smoke.phase = 2
                    return
                }
                if (smoke.phase === 2) {
                    if (!Player.playing || Player.paused || !find(smoke.mainWindow.contentItem, "queueVisualizationToggle")) return
                    check(!AudioOutput.shown, "Queue opened equipment inspection")
                    check(item("queueVisualizationToggle").enabled, "Queue visualization is unavailable")
                    item("queueVisualizationToggle").clicked()
                    smoke.selectMode(0)
                    smoke.phase = 3
                    return
                }
                if (smoke.phase === 3) {
                    if (smoke.ticks - smoke.began < 20) return
                    check(Player.playing && !Player.paused, "Capture stopped")
                    check(smoke.counts[smoke.names[smoke.mode]] - smoke.frames > 3, "No fresh stream for mode " + smoke.mode)
                    check(item("queueVisualization").item.capturing, "Visible mode stopped capturing")
                    const path = item("queueSignalPathHost").item.document
                    check(path.stages.length === 4, "Queue signal path lacks current engine stages")
                    console.log("SMOKE queue visualizer mode=" + smoke.mode + " fresh=" + (smoke.counts[smoke.names[smoke.mode]] - smoke.frames))
                    if (smoke.mode < 6) { smoke.selectMode(smoke.mode + 1); return }
                    smoke.capturing = true
                    smoke.mainWindow.contentItem.grabToImage(result => {
                        if (!result.saveToFile(smoke.outputDirectory + "/queues.png")) {
                            console.error("SMOKE FAIL queue visualizer: cannot save screenshot")
                            Qt.exit(1)
                            return
                        }
                        item("queueVisualizationToggle").clicked()
                        smoke.capturing = false
                        smoke.began = smoke.ticks; smoke.phase = 4
                    })
                    return
                }
                if (smoke.phase === 4) {
                    if (smoke.ticks - smoke.began < 5) return
                    check(item("queueVisualization").item === null, "Disabled visualization retained its renderer")
                    smoke.parkedFrames = Object.values(smoke.counts).reduce((a, b) => a + b, 0)
                    smoke.began = smoke.ticks; smoke.phase = 5
                    return
                }
                if (smoke.ticks - smoke.began < 8) return
                check(smoke.parkedFrames === Object.values(smoke.counts).reduce((a, b) => a + b, 0), "Hidden visualization still publishes frames")
                check(!AudioOutput.shown, "Signal path changed equipment-panel state")
                Player.clearQueue()
                console.log("SMOKE queue visualizer completed")
                Qt.quit()
            } catch (error) {
                console.error("SMOKE FAIL queue visualizer: " + error)
                Qt.exit(1)
            }
        }
    }
}
