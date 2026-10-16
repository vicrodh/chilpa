import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.graphics
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/immersive/qml/Patterns" as Patterns
import "qrc:/qt/qml/app/chilpa/immersive/qml/Components" as Parts

Window {
    id: window
    width: 960
    height: 640
    visible: true
    color: "black"
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property string expectedBackend: "EXPECTED_BACKEND"
    property var modes: [1, 2, 3, 4, 5, 7, 8, -7, -8]
    property var names: ["plasma", "tunnel", "aurora", "ribbon", "linebed", "ambient", "tunnel-flow", "goniometer", "oscilloscope"]
    property int modeIndex: 0
    property int frame: 0
    property int tick: 0
    property int saved: 0
    property bool capturing: false
    property bool passedLifecycle: false
    property real frozenClock: 0

    QtObject {
        id: signal
        property string packJson: ""
        property var bars: []
        property var linebedHeights: null
        property var ribbonFrame: null
        property var goniometer: []
        property var oscilloscope: []
        property real stereoCorrelation: 0.63
        property int sampleRateHz: 48000
        property int bitDepth: 24
        property int ribbonEpoch: 1
        property int ribbonSequence: 0
        property var ribbonHistory: new Uint8Array(512 * 2048)
        property int lastColumn: 0
        function ribbonSnapshot() {
            const snapshot = new ArrayBuffer(20 + 512 * 2048)
            new DataView(snapshot).setUint32(0, lastColumn, true)
            new DataView(snapshot).setUint32(4, ribbonEpoch, true)
            new DataView(snapshot).setUint32(12, ribbonSequence, true)
            const bytes = new Uint8Array(snapshot)
            bytes.set(ribbonHistory, 20)
            return snapshot
        }
    }

    Patterns.ShaderSceneLayer {
        id: layer
        anchors.fill: parent
        feed: signal
        running: true
        opened: true
        visible: window.modes[window.modeIndex] > 0
    }
    Loader {
        id: scope
        anchors.fill: parent
        anchors.margins: 48
        active: window.modes[window.modeIndex] < 0
        sourceComponent: Parts.ScopePanel {
            feed: signal
            scopeMode: window.modes[window.modeIndex] === -7 ? 0 : 1
        }
    }

    function fail(message) {
        console.error("SMOKE FAIL immersive scenes: " + message)
        Qt.exit(1)
    }
    function check(condition, message) { if (!condition) fail(message) }
    function synthesize() {
        const t = tick * 0.13
        const beat = Math.pow(Math.max(0, Math.cos(t * 1.7)), 8)
        const energy = [0.35 + beat * 0.5, 0.4 + Math.sin(t) * 0.25, 0.35, 0.25]
        signal.packJson = JSON.stringify({phase: tick * 0.045, beat: beat, beatAbove: beat,
            level: 0.45, levelSmooth: 0.38, transient: beat, energyLo: energy,
            energyHi: [0.3, 0.64, 0, 0], bandsLo: [0.8, 0.4, beat, 0.6], bandsHi: [0.3, 0.6, 0.2, 0.5]})
        signal.bars = Array.from({length: 16}, (_, i) => 0.4 + 0.35 * Math.sin(t + i * 0.4))
        const heights = new Float32Array(256 * 200)
        for (let row = 0; row < 200; ++row)
            for (let column = 0; column < 256; ++column)
                heights[row * 256 + column] = 0.1 + 66 * Math.pow(Math.max(0, Math.sin(column * 0.045 + row * 0.07 + t)), 4)
        signal.linebedHeights = heights.buffer
        if (modes[modeIndex] === 4) {
            const ribbon = new ArrayBuffer(532)
            const bytes = new Uint8Array(ribbon)
            const data = new DataView(ribbon)
            // The real feed runs only for Ribbon and follows a monotonic
            // track cursor. Keep a stable opening tone while the GPU mounts.
            const column = (frame - 1) * 12
            const ribbonTime = Math.max(8, frame) * 0.13
            data.setUint32(0, column, true)
            if (frame === 12) {
                // The first reset row is deliberately replaced in this GUI
                // turn. The next row still carries the new epoch.
                signal.ribbonEpoch += 1
                signal.ribbonSequence += 1
                const reset = new ArrayBuffer(532)
                const header = new DataView(reset)
                header.setUint32(0, column, true)
                header.setUint32(4, signal.ribbonEpoch, true)
                header.setUint32(12, signal.ribbonSequence, true)
                signal.ribbonFrame = reset
                signal.ribbonHistory.fill(0)
                signal.lastColumn = 0
            }
            signal.ribbonSequence += 1
            data.setUint32(4, signal.ribbonEpoch, true)
            data.setUint32(12, signal.ribbonSequence, true)
            for (let i = 0; i < 512; ++i)
                bytes[i + 20] = Math.round(255 * Math.pow(Math.max(0, Math.sin(i * 0.027 + ribbonTime)), 3))
            if (frame === 1) {
                signal.ribbonHistory.fill(0)
                signal.lastColumn = 0
            }
            const first = column > signal.lastColumn ? signal.lastColumn + 1 : column
            for (let previous = first; previous <= column; ++previous)
                signal.ribbonHistory.set(bytes.subarray(20), previous * 512)
            signal.lastColumn = column
            signal.ribbonFrame = ribbon
        }
        signal.oscilloscope = Array.from({length: 512}, (_, i) => 0.65 * Math.sin(i / 511 * Math.PI * 4 + t * 0.1))
        const points = []
        for (let i = 0; i < 256; ++i) {
            points.push(0.64 * Math.sin(i * 0.049 + t * 0.7))
            points.push(0.76 * Math.sin(i * 0.049 + t * 0.7 + 0.8))
        }
        signal.goniometer = points
    }
    function capture(suffix) {
        capturing = true
        const item = modes[modeIndex] > 0 ? layer : scope
        const path = outputDirectory + "/" + names[modeIndex] + "-" + suffix + ".png"
        item.grabToImage(result => {
            check(result.saveToFile(path), "cannot save " + path)
            ++saved
            capturing = false
        })
    }
    function lifecycle() {
        if (modes[modeIndex] < 0)
            return
        check(layer.active && layer.holding, "active scene must hold the shared pulse")
        layer.running = false
        frozenClock = layer.timeS
        check(!layer.holding, "paused scene must release the shared pulse")
    }
    function nextMode() {
        if (modes[modeIndex] > 0) {
            check(layer.timeS === frozenClock, "paused scene changed its clock")
            layer.visible = false
            check(!layer.active && !layer.holding, "hidden scene did not stop")
            layer.running = true
        }
        console.log("SMOKE scene rendered " + names[modeIndex])
        ++modeIndex
        frame = 0
        if (modeIndex >= modes.length) {
            check(saved === 22, "missing captures: " + saved)
            Immersive.setAvailable(false)
            check(Immersive.scene === 0, "low GPU did not restore a panel")
            check(!layer.available, "low GPU left scenes available")
            Immersive.open = false
            console.log("SMOKE immersive scenes completed: 7 scenes, 2 scopes, pause, hidden, low GPU")
            Qt.quit()
            return
        }
        layer.visible = modes[modeIndex] > 0
        Immersive.chooseScene(Math.max(0, modes[modeIndex]))
    }
    Component.onCompleted: {
        check(GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Null, "GPU scene graph required")
        Immersive.setAvailable(true)
        Immersive.open = true
        Immersive.chooseScene(1)
        Player.duration = 180
        clock.start()
    }
    Timer {
        id: clock
        interval: 33
        repeat: true
        onTriggered: {
            if (window.capturing)
                return
            ++window.frame
            ++window.tick
            if (window.frame === 1 && window.modeIndex === 0) {
                const expected = window.expectedBackend === "vulkan" ? GraphicsInfo.Vulkan : GraphicsInfo.OpenGL
                window.check(window.GraphicsInfo.api === expected, "requested backend was not selected: " + window.expectedBackend + " / " + window.GraphicsInfo.api)
                console.log("SMOKE renderer " + window.expectedBackend)
            }
            if (window.frame < 45)
                window.synthesize()
            if (window.frame === 20)
                window.capture("a")
            if (window.frame === 40)
                window.capture("b")
            if (window.frame === 46)
                window.lifecycle()
            if (window.frame === 47 && (window.modes[window.modeIndex] === 4 || window.modes[window.modeIndex] === 5))
                window.capture("paused")
            if (window.frame === 48 && window.modes[window.modeIndex] > 0)
                layer.visible = false
            if (window.frame === 50 && window.modes[window.modeIndex] > 0) {
                layer.visible = true
                window.check(layer.active && !layer.holding, "paused restore must show without restarting the clock")
                if (window.modes[window.modeIndex] === 4) {
                    // Simulate two queued rows from before the snapshot in
                    // the same epoch, before the new renderer synchronizes.
                    // Neither may replace the snapshot mailbox or its pixels.
                    for (const child of layer.children) {
                        if (!child.item || !child.item.pulseTick)
                            continue
                        for (const sequence of [1, 2]) {
                            const stale = new ArrayBuffer(532)
                            const header = new DataView(stale)
                            header.setUint32(4, signal.ribbonEpoch, true)
                            header.setUint32(12, sequence, true)
                            new Uint8Array(stale).fill(255, 20)
                            child.item.pulseTick(stale, layer.energyHi)
                        }
                    }
                }
            }
            if (window.frame === 52 && (window.modes[window.modeIndex] === 4 || window.modes[window.modeIndex] === 5))
                window.capture("restored")
            if (window.frame === 54)
                window.nextMode()
        }
    }
}
