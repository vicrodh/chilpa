import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/immersive/qml/Patterns" as Panels

Window {
    id: window
    width: 800
    height: 684
    visible: true
    color: "#101820"
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property string expectedBackend: "EXPECTED_BACKEND"
    property url localSource: "LOCAL_SOURCE"
    property var tile: null
    property var decoded: null
    property var baseline: null
    property int ticks: 0
    property int frames: 0
    property int captures: 0
    property bool capturing: false
    property real smallest: 2
    property real largest: 0
    Rectangle { anchors.fill: parent; color: window.color }
    Panels.AlbumReactivePanel { id: panel; anchors.fill: parent }

    function fail(message) { console.error("SMOKE FAIL reactive artwork: " + message); Qt.exit(1); throw new Error(message) }
    function check(value, message) { if (!value) fail(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) {
            const found = find(child, predicate)
            if (found) return found
        }
        return null
    }
    function stable() {
        if (!baseline) return
        check(tile.ready && decoded.status === Image.Ready, "a pulse unloaded the cover")
        check(tile.width === baseline.width && tile.height === baseline.height && tile.decode === baseline.decode,
              "a pulse resized the decoded artwork")
        check(decoded.source.toString() === baseline.source && decoded.sourceSize.width === baseline.size,
              "a pulse changed the source or decode resolution")
    }
    Connections { target: window.tile; function onReadyChanged() { window.stable() } }
    FrameAnimation {
        running: window.baseline !== null
        onTriggered: { ++window.frames; window.stable() }
    }
    Timer {
        interval: 100
        running: window.captures < 8
        repeat: true
        onTriggered: {
            if (!window.baseline) {
                Player.art = window.localSource
                Immersive.open = true
                Visualizer.energy = new Array(5).fill(0)
                if (!window.tile) window.tile = window.find(panel, item => item.decode !== undefined && item.ready !== undefined)
                if (!window.tile || !window.tile.ready || panel.ownWidth !== 320) return
                const expected = window.expectedBackend === "vulkan" ? GraphicsInfo.Vulkan : GraphicsInfo.OpenGL
                window.check(window.GraphicsInfo.api === expected, "the requested GPU backend was not selected")
                window.decoded = window.find(window.tile, item => item.sourceSize !== undefined)
                window.baseline = {width:window.tile.width, height:window.tile.height, decode:window.tile.decode,
                    source:window.decoded.source.toString(), size:window.decoded.sourceSize.width}
                return
            }
            if (window.capturing) return
            ++window.ticks
            // I inject energy explicitly to cross decode boundaries repeatedly;
            // real decoder and analyzer coverage belongs to the musical smoke.
            Visualizer.energy = new Array(5).fill(Math.floor(window.ticks / 8) % 2)
            window.stable()
            window.smallest = Math.min(window.smallest, panel.artScale)
            window.largest = Math.max(window.largest, panel.artScale)
            if (window.ticks % 8 !== 7) return
            const center = window.tile.mapToItem(window.contentItem, window.tile.width / 2, window.tile.height / 2)
            const record = {file:"reactive-" + window.captures + ".png", x:center.x, y:center.y, scale:panel.artScale}
            window.capturing = true
            window.contentItem.grabToImage(result => {
                window.check(result.saveToFile(window.outputDirectory + "/" + record.file), "cannot save artwork capture")
                console.log("SMOKE reactive capture " + JSON.stringify(record))
                ++window.captures
                window.capturing = false
                if (window.captures < 8) return
                window.check(window.largest - window.smallest > 0.15 && window.frames > 20, "the stable artwork did not breathe")
                console.log("SMOKE reactive artwork completed: frames=" + window.frames + ", captures=" + window.captures
                    + ", decode=" + window.baseline.decode + ", injected_energy=true")
                Qt.quit()
            })
        }
    }
    Timer { interval: 15000; running: true; onTriggered: window.fail("timed out") }
}
