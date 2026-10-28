import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The playing row's mark: still bars unless Appearance asks for the moving
// ones, which exist only while the track runs and hold the pulse only then.
TestCase {
    id: test
    name: "PlayingMark"
    when: windowShown
    visible: true
    width: 200
    height: 100

    PlayingIndicator { id: mark; objectName: "mark" }
    ListRowSurface { id: surface; y: 40; width: 200; height: 40 }
    Component { id: barsFactory; PlayingBars { active: false } }
    Component { id: stageFactory; Rectangle { x: 80; color: "black" } }
    Component { id: windowFactory; Window { width: 200; height: 100; visible: true } }
    Component { id: rowFactory; Row { x: 20; y: 10; Rectangle { width: 20; height: 20; color: "black" } } }

    function cleanup() {
        Prefs.animatedIndicator = false
        mark.running = false
        mark.visible = true
        surface.edge = false
        Pulse.automatic = true
        Pulse.period = 33
    }

    SignalSpy { id: toggled; target: mark; signalName: "toggled" }

    function test_still_by_default() {
        mark.running = true
        compare(mark.moving, false)
        compare(mark.children[0].item, null, "no moving bars while the option is off")
        compare(Pulse.held, 0)
    }

    function test_the_still_mark_is_circle_play_and_pause_under_the_pointer() {
        toggled.clear()
        mark.running = true
        const still = findChild(mark, "playingMarkStill")
        verify(still !== null)
        compare(still.glyph, Lucide.circlePlay)
        compare(still.font.family, Lucide.font)
        mouseMove(still, still.width / 2, still.height / 2)
        tryCompare(still, "glyph", Lucide.circlePause)
        mouseClick(still)
        compare(toggled.count, 1, "a click asks to pause")
        mark.running = false
        compare(still.glyph, Lucide.circlePlay, "a paused track offers play")
        mouseMove(test, 190, 90)
        mark.visible = false
        tryVerify(() => findChild(mark, "playingMarkStill") === null, 1000, "a hidden mark builds nothing")
    }

    function test_moves_only_while_running() {
        Prefs.animatedIndicator = true
        compare(mark.children[0].item, null, "a paused track builds no bars")
        compare(Pulse.held, 0)
        mark.running = true
        verify(mark.children[0].item !== null, "the running track gets the moving bars")
        compare(Pulse.held, 1, "the bars hold the pulse")
        const before = mark.children[0].item.t
        tryVerify(() => mark.children[0].item.t !== before, 500, "the bars move on the pulse")
        mark.running = false
        compare(mark.children[0].item, null)
        compare(Pulse.held, 0, "pausing lets the pulse go")
    }

    function test_hidden_mark_builds_nothing() {
        Prefs.animatedIndicator = true
        mark.running = true
        mark.visible = false
        compare(mark.children[0].item, null)
        compare(Pulse.held, 0)
    }


    // QBZ's pace (immersive/EqualizerBars.qml): every pulse edge moves the
    // loop by one period of its 800 ms, about 24 samples a loop at 33 ms,
    // on the normal clock and never the slow one.
    function test_every_pulse_edge_moves_the_loop_by_one_period() {
        Pulse.automatic = false
        const bars = createTemporaryObject(barsFactory, test, {active: true})
        compare(Pulse.held, 1)
        compare(Pulse.slowHeld, 0, "the bars never take the slow pulse")
        const before = bars.t
        Pulse.ms += Pulse.period
        fuzzyCompare(bars.t, (before + 33 / 800) % 1, 0.00001)
        Pulse.ms += Pulse.period
        fuzzyCompare(bars.t, (before + 66 / 800) % 1, 0.00001)
        bars.active = false
        compare(Pulse.held, 0)
    }

    function test_hidden_window_releases_the_pulse() {
        const window = createTemporaryObject(windowFactory, test)
        const bars = createTemporaryObject(barsFactory, window.contentItem, {x: 20, y: 20, active: true})
        tryCompare(bars, "holding", true)
        window.hide()
        tryCompare(bars, "holding", false)
        compare(Pulse.held, 0)
        window.show()
        tryCompare(bars, "holding", true)
        bars.active = false
    }

    function test_four_bars_keep_the_wave_geometry_and_one_pulse() {
        const bars = createTemporaryObject(barsFactory, test, {barScale: 1.5, tint: Qt.rgba(0.3, 0.6, 0.8, 0.5)})
        // One arm draws: the quad where the scene graph has shaders, the
        // four rectangles where it has none; the other builds nothing.
        const quad = findChild(bars, "playingBarsShader")
        const rectangles = findChild(bars, "playingBarsRectangles")
        verify(quad !== null && rectangles !== null)
        compare(quad.visible, bars.shaderOk, "the quad draws where there are shaders")
        compare(rectangles.active, !bars.shaderOk, "the rectangles draw where there are none")
        compare(quad.width, bars.width + 2 * bars.barScale)
        compare(quad.height, bars.height + 2 * bars.barScale)
        compare(quad.radius, bars.barScale)
        compare(quad.tint, bars.tint)
        compare(quad.dims, Qt.size(bars.width, bars.height))
        compare(quad.geometry, Qt.vector2d(Metrics.eqBar * bars.barScale, Metrics.eqStep * bars.barScale))
        const amplitudes = [0.6, 1, 0.4, 0.8], phases = [0, 0.25, 0.125, 0.375]
        for (const t of [0, 0.125, 0.25, 0.5, 0.875]) {
            bars.t = t
            fuzzyCompare(quad.phase, t, 0.00001, "the quad takes the phase as its uniform")
            if (bars.shaderOk)
                continue
            compare(rectangles.item.children.length, 4)
            for (let i = 0; i < 4; ++i) {
                const bar = rectangles.item.children[i]
                fuzzyCompare(bar.height, bars.height * amplitudes[i] * bars.wave(phases[i]), 0.00001)
                fuzzyCompare(bar.y + bar.height, bars.height, 0.00001)
                compare(bar.width, Metrics.eqBar * bars.barScale)
                compare(bar.radius, bars.barScale)
                compare(bar.color, bars.tint)
            }
        }
        compare(Pulse.held, 0)
        bars.active = true
        compare(Pulse.held, 1)
        const before = bars.t
        tryVerify(() => bars.t !== before, 500)
        bars.visible = false
        compare(Pulse.held, 0)
        bars.visible = true
        compare(Pulse.held, 1)
        bars.active = false
        compare(Pulse.held, 0)
    }

    function test_edge_mark() {
        const edge = surface.children[0]
        compare(edge.visible, false)
        surface.edge = true
        compare(edge.visible, true)
        compare(edge.width, Metrics.playingEdge)
        compare(edge.height, surface.height - 2 * Metrics.playingEdgeInset)
    }
}
