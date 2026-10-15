import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.dsp
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/immersive/qml/Screens" as Views

Window {
    id: window
    width: 960
    height: 720
    visible: true
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property bool withDsp: "WITH_DSP"
    property bool started: false
    property bool waiting: false
    property bool capturing: false
    property int tick: 0
    property int stage: 0
    property int began: 0
    property int beganBars: 0
    property var stages: []
    property var modes: [
        {name:"goniometer", mode:7, scene:0}, {name:"oscilloscope", mode:8, scene:0},
        {name:"linebed", mode:0, scene:5}, {name:"plasma", mode:0, scene:1},
        {name:"tunnel", mode:0, scene:2}, {name:"aurora", mode:0, scene:3},
        {name:"ribbon", mode:0, scene:4}, {name:"ambient", mode:0, scene:7},
        {name:"tunnel-flow", mode:0, scene:8}
    ].concat(withDsp ? [{name:"goniometer-dsp", mode:7, scene:0, dsp:true},
                       {name:"linebed-dsp", mode:0, scene:5, dsp:true}] : [])
    property var frames: ({bars:0, pack:0, goniometer:0, oscilloscope:0, linebed:0, ribbon:0})
    property var peaks: ({wave:0, goniometer:0, oscilloscope:0, linebed:0, level:0})
    readonly property bool playing: Player.playing && !Player.paused

    QtObject { id: controls; property bool immersiveOpen: false; signal immersiveAsked() }
    SettingsPage { id: audio; section: "audio" }
    Views.ImmersiveView { anchors.fill: parent; window: window; controls: controls }

    function peak(values) {
        let value = 0
        for (let index = 0; index < values.length; ++index) value = Math.max(value, Math.abs(values[index]))
        return value
    }
    function fail(message) {
        console.error("SMOKE FAIL live visualizer: " + message)
        console.log("SMOKE live metrics " + JSON.stringify({frames:frames, peaks:peaks, path:Player.path, playing:playing, error:Player.error, started:started, waiting:waiting, stage:stage}))
        Player.clearQueue()
        Qt.exit(1)
    }
    function check(condition, message) { if (!condition) fail(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) {
            const match = find(child, predicate)
            if (match) return match
        }
        return null
    }
    function selectStage() {
        began = tick
        beganBars = frames.bars
        const next = modes[stage]
        if (next.dsp) {
            Dsp.refresh()
            const state = JSON.parse(Dsp.snapshot || "{}")
            check(Dsp.available && state.providers.length === 1, "the packaged DSP is not available")
            Dsp.command("choose", JSON.stringify({plugin:state.providers[0].id, preset:"hd650"}))
        }
        Immersive.setAvailable(true)
        Immersive.openView(0, next.mode, 0)
        Immersive.chooseScene(next.scene)
        console.log("SMOKE live stage " + next.name)
    }
    Connections {
        target: Visualizer
        function onBarsChanged() { ++window.frames.bars }
        function onWaveformChanged() { window.peaks.wave = Math.max(window.peaks.wave, window.peak(Visualizer.waveform)) }
        function onPackJsonChanged() {
            ++window.frames.pack
            const pack = JSON.parse(Visualizer.packJson || "{}")
            window.peaks.level = Math.max(window.peaks.level, pack.level || 0)
        }
        function onGoniometerChanged() {
            ++window.frames.goniometer
            window.peaks.goniometer = Math.max(window.peaks.goniometer, window.peak(Visualizer.goniometer))
        }
        function onOscilloscopeChanged() {
            ++window.frames.oscilloscope
            window.peaks.oscilloscope = Math.max(window.peaks.oscilloscope, window.peak(Visualizer.oscilloscope))
        }
        function onLinebedHeightsChanged() {
            ++window.frames.linebed
            window.peaks.linebed = Math.max(window.peaks.linebed, window.peak(new Float32Array(Visualizer.linebedHeights)))
        }
        function onRibbonFrameChanged() { ++window.frames.ribbon }
    }
    Timer {
        interval: 50
        running: window.stage < window.modes.length
        repeat: true
        onTriggered: {
            if (window.stage >= window.modes.length) return
            ++window.tick
            if (!window.started) {
                // I use the restored player queue: this generated WAV has no library row.
                if (!Player.path.endsWith("fixture.wav")) return
                audio.set("audio.device", "file:capture")
                window.started = true
                Player.toggle()
                window.waiting = true
                return
            }
            if (window.capturing) return
            if (window.waiting) {
                if (!window.playing) return
                window.waiting = false
                window.selectStage()
                return
            }
            if (window.tick - window.began < 42) return
            const current = window.modes[window.stage]
            window.check(window.playing, "playback stopped during " + current.name)
            window.check(window.frames.bars - window.beganBars > 3 && window.frames.pack > 3 && window.peaks.wave > 0.005,
                         "the real PCM tap did not reach the Qt visualizer in " + current.name)
            if (current.mode === 7) window.check(window.peaks.goniometer > 0.001, "goniometer has no music")
            if (current.name === "oscilloscope") window.check(window.peaks.oscilloscope > 0.001, "oscilloscope has no music")
            if (current.scene === 5) window.check(window.frames.linebed > 3 && window.peaks.linebed > 0, "Line Bed has no music")
            if (current.dsp) { Dsp.refresh(); window.check(Dsp.active, "the music tap was not checked with active DSP") }
            if (current.scene > 0) {
                const layer = window.find(window.contentItem, item => typeof item.rhiPack === "function" && item.active)
                window.check(!!layer && layer.moving && layer.level > 0.0001,
                             "the actual scene did not consume native music levels in " + current.name)
            }
            window.stages.push({name:current.name, freshBars:window.frames.bars - window.beganBars})
            window.capturing = true
            window.contentItem.grabToImage(result => {
                window.check(result.saveToFile(window.outputDirectory + "/" + current.name + ".png"), "cannot save " + current.name)
                window.capturing = false
                ++window.stage
                if (window.stage < window.modes.length) {
                    window.selectStage()
                    return
                }
                console.log("SMOKE live metrics " + JSON.stringify({frames:window.frames, peaks:window.peaks, stages:window.stages}))
                Player.clearQueue()
                Immersive.open = false
                console.log("SMOKE live visualizer completed: real decoder, capture sink, tap, analyzer, native bridge and views")
                Qt.quit()
            })
        }
    }
    Timer { interval: 60000; running: true; onTriggered: window.fail("timeout") }
}
