import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "CompactVisualizer"
    when: windowShown
    visible: true
    width: 340
    height: 120
    QtObject {
        id: feed
        property var bars: []
        property var waveform: []
        property var energy: []
        property var goniometer: []
        property var oscilloscope: []
        property bool presented: false
        property int selected: -1
        property int claims: 0
        function setCompact(active, mode) { claims++; presented = active; selected = mode }
    }
    Rectangle {
        id: stage; width: 320; height: 42; color: "black"
        CompactVisualizer { id: band; anchors.fill: parent; feed: feed }
    }
    SignalSpy { id: cycle; target: band; signalName: "cycleRequested" }

    function init() {
        Pulse.automatic = false
        band.active = false
        band.visible = true
        band.mode = 0
        feed.bars = []; feed.waveform = []; feed.energy = []
        cycle.clear()
    }
    function cleanup() { band.active = false; Pulse.automatic = true }
    function pulse(count) { for (let i = 0; i < count; ++i) Pulse.ms += Pulse.period }

    function test_disabled_and_hidden_keep_no_renderer_or_pulse() {
        verify(!feed.presented)
        compare(findChild(band, "compactVisualizerShader"), null)
        compare(Pulse.held, 0)
        band.active = true
        verify(feed.presented)
        feed.bars = Array(16).fill(0.7)
        compare(Pulse.held, 1, "one shared pulse subscription for the whole band")
        pulse(1)
        band.visible = false
        verify(!feed.presented)
        tryVerify(() => findChild(band, "compactVisualizerShader") === null)
        compare(Pulse.held, 0)
        band.visible = true
        verify(feed.presented, "the same retained item resumes after showing")
        verify(findChild(band, band.gpuAvailable ? "compactVisualizerShader" : "compactVisualizerFallback") !== null)
    }
    function test_modes_select_one_feed_and_silence_self_parks() {
        band.active = true
        pulse(1)
        compare(Pulse.held, 0)
        feed.bars = Array(16).fill(0)
        pulse(1)
        compare(Pulse.held, 0, "repeated silence produces no continuing frames")
        band.mode = 2
        compare(feed.selected, 2)
        pulse(1)
        feed.bars = Array(16).fill(0.9)
        compare(Pulse.held, 0, "an unselected stream cannot animate")
        feed.energy = [0.1, 0.4, 0.7, 0.5, 0.3]
        compare(Pulse.held, 1)
        pulse(30)
        compare(Pulse.held, 0, "a steady target converges and releases the clock")
        band.mode = 5
        feed.bars = Array(16).fill(0.8)
        pulse(20)
        feed.bars = Array(16).fill(0)
        pulse(60)
        compare(Pulse.held, 0, "peak marks expire instead of holding a timer")
    }
    function test_peak_and_squares_draw_discrete_cells() {
        band.active = true
        band.topColor = "white"
        band.bottomColor = "white"
        for (const mode of [5, 6]) {
            band.mode = mode
            feed.bars = Array(16).fill(0.9)
            pulse(30)
            waitForRendering(band)
            const picture = grabImage(stage)
            const x = Math.floor(band.width / 28 / 2)
            verify(picture.red(x, 35) > 200, "a lit cell is visible")
            verify(picture.red(x, 29) < 100, "the rows have a dark gap, not a continuous bar: " + picture.red(x, 29))
            feed.bars = Array(16).fill(0)
            pulse(60)
            waitForRendering(band)
            const silent = grabImage(stage)
            for (let y = 0; y < band.height; ++y)
                verify(silent.red(x, y) < 100, "the entire silent column clears, including the last peak at y=" + y)
        }
    }
    Component { id: passiveMaker; CompactSpectrumState { source: feed.bars } }
    function test_missing_frame_parks_and_accepts_the_first_valid_frame() {
        feed.bars = undefined
        const claims = feed.claims
        const state = createTemporaryObject(passiveMaker, stage, {active: true})
        verify(state !== null)
        state.receive()
        compare(state.targetValues.length, 0)
        pulse(1)
        compare(Pulse.held, 0, "a missing initial frame does not keep the clock running")
        feed.bars = Array(16).fill(0.8)
        compare(state.targetValues.length, 16)
        state.tick()
        fuzzyCompare(state.values[0], 0.36, 0.000001, "the first valid frame retains the existing easing")
        pulse(30)
        compare(Pulse.held, 0)
        compare(feed.claims, claims, "receiving the first frame never acquires capture")
    }

    function test_passive_consumer_never_changes_an_existing_capture_claim() {
        band.active = true
        const calls = feed.claims
        const passive = passiveMaker.createObject(stage, {active: true})
        verify(passive !== null)
        for (const mode of [0, 5, 6]) {
            passive.mode = mode
            passive.active = false
            passive.active = true
        }
        passive.destroy()
        wait(0)
        compare(feed.claims, calls, "a passive consumer never acquires or releases capture")
        verify(feed.presented, "the original owner's claim survives destruction")
    }

    function test_cycle_preserves_the_reference_order_and_appends_variants() {
        for (let mode = 0; mode < 7; ++mode) {
            band.mode = mode
            const next = band.nextMode()
            if (mode === 2) compare(next, 5, "without a scope renderer the cycle skips scopes")
            else compare(next, (mode + 1) % 7)
        }
        band.mode = 5
        mouseClick(band)
        compare(cycle.count, 1)
        compare(cycle.signalArguments[0][0], 6)
    }
}
