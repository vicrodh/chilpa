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

    function test_sampling_preserves_elapsed_phase_and_shared_edges() {
        Pulse.automatic = false
        Pulse.ms = 0
        const first = createTemporaryObject(barsFactory, test, {active: true, shaderFailed: true})
        Pulse.ms += Pulse.period
        compare(first.t, 0)
        const second = createTemporaryObject(barsFactory, test, {active: true, shaderFailed: true})
        Pulse.ms += Pulse.period
        compare(first.t, 0)
        compare(second.t, 0)
        for (let i = 0; i < 6; ++i) Pulse.ms += Pulse.period
        fuzzyCompare(first.t, 264 / Metrics.eqLoop, 0.00001)
        fuzzyCompare(second.t, 231 / Metrics.eqLoop, 0.00001)
        for (let i = 0; i < 24; ++i) Pulse.ms += Pulse.period
        fuzzyCompare(first.t, (32 * Pulse.period / Metrics.eqLoop) % 1, 0.00001)
        first.visible = false
        second.active = false
        compare(Pulse.held, 0)
        const stopped = first.t
        Pulse.ms += Pulse.period
        compare(first.t, stopped)
        first.visible = true
        for (let i = 0; i < 8; ++i) Pulse.ms += Pulse.period
        fuzzyCompare(first.t, (stopped + 231 / Metrics.eqLoop) % 1, 0.00001)
    }

    function test_fallback_never_exceeds_four_scene_updates_a_second() {
        Pulse.automatic = false
        for (const period of [10, 33, 99, 100, 200]) {
            Pulse.period = period
            Pulse.ms = 0
            const bars = createTemporaryObject(barsFactory, test, {active: true, shaderFailed: true})
            let changes = 0
            for (let elapsed = period; elapsed <= 3000; elapsed += period) {
                const before = bars.t
                Pulse.ms += period
                if (before !== bars.t) changes++
            }
            verify(changes > 0 && changes <= 12)
            bars.active = false
            const held = bars.t
            Pulse.ms += period
            compare(bars.t, held)
        }
    }

    function test_sampling_adapts_to_the_shared_period() {
        Pulse.automatic = false
        for (const period of [10, 33, 100, 200]) {
            Pulse.period = period
            Pulse.ms = 0
            const bars = createTemporaryObject(barsFactory, test, {active: true, shaderFailed: true})
            const steps = Math.max(1, Math.ceil(Metrics.eqSceneFrame / period))
            for (let i = 1; i < steps; ++i) {
                Pulse.ms += period
                compare(bars.t, 0)
            }
            Pulse.ms += period
            fuzzyCompare(bars.t, steps * period / Metrics.eqLoop, 0.00001)
            bars.active = false
            compare(Pulse.held, 0)
        }
    }

    function test_period_changes_preserve_an_already_queued_edge() {
        Pulse.automatic = false
        Pulse.ms = 0
        const bars = createTemporaryObject(barsFactory, test, {active: true, shaderFailed: true})
        Pulse.ms = 33
        Pulse.period = 99
        Pulse.ms = 66
        fuzzyCompare(bars.t, 66 / Metrics.eqLoop, 0.00001)
        Pulse.ms = 165
        fuzzyCompare(bars.t, 66 / Metrics.eqLoop, 0.00001)
        Pulse.ms = 297
        fuzzyCompare(bars.t, 297 / Metrics.eqLoop, 0.00001)
        bars.active = false
        Pulse.ms = 86399973
        Pulse.period = 33
        bars.active = true
        Pulse.ms = 6
        fuzzyCompare(bars.t, 330 / Metrics.eqLoop, 0.00001)
    }

    function test_native_surface_yields_to_stacking_clipping_and_opacity() {
        const stage = createTemporaryObject(stageFactory, test, {x: 40, y: 20, width: 100, height: 60})
        const bars = createTemporaryObject(barsFactory, stage, {x: 10, y: 10, active: true})
        if (!bars.supportsShaders(GraphicsInfo.api)) skip("The renderer uses the rectangle fallback")
        const native = findChild(bars, "playingBarsSurface")
        tryVerify(() => native.surface !== null, 2000)
        const cover = createTemporaryObject(stageFactory, stage, {x: 8, y: 8, width: 24, height: 20, z: 1, color: "green"})
        tryCompare(native, "surface", null)
        verify(findChild(bars, "playingBarsShader") !== null)
        compare(Pulse.held, 1)
        cover.visible = false
        tryVerify(() => native.surface !== null)
        stage.clip = true
        bars.x = -1
        tryCompare(native, "surface", null)
        bars.x = 10
        tryVerify(() => native.surface !== null)
        stage.opacity = 0.5
        tryCompare(native, "surface", null)
        stage.opacity = 1
        tryVerify(() => native.surface !== null)
        stage.scale = 1.1
        tryCompare(native, "surface", null)
        stage.scale = 1
        tryVerify(() => native.surface !== null)
        stage.layer.enabled = true
        tryCompare(native, "surface", null)
        stage.layer.enabled = false
        tryVerify(() => native.surface !== null)
        stage.x += 5
        tryCompare(native.surface, "x", stage.x + bars.x - bars.barScale)
        bars.active = false
        compare(native.surface, null)
        compare(Pulse.held, 0)
    }

    function test_native_surfaces_are_bounded_and_released() {
        const stage = createTemporaryObject(stageFactory, test, {x: 0, width: 180, height: 30})
        const marks = []
        for (let i = 0; i < 5; ++i)
            marks.push(createTemporaryObject(barsFactory, stage, {x: 4 + 32 * i, y: 4, active: true}))
        if (!marks[0].supportsShaders(GraphicsInfo.api)) skip("The renderer uses the rectangle fallback")
        const native = marks.map(item => findChild(item, "playingBarsSurface"))
        tryVerify(() => native.filter(item => item.surface !== null).length === 4)
        const fallback = native.findIndex(item => item.surface === null)
        const first = native.findIndex(item => item.surface !== null)
        marks[first].active = false
        tryVerify(() => native[fallback].surface !== null)
        marks[fallback].barScale = 5
        tryCompare(native[fallback], "surface", null)
        for (const item of marks) item.active = false
        compare(Pulse.held, 0)
    }

    function test_hidden_window_releases_the_pulse() {
        const window = createTemporaryObject(windowFactory, test)
        const bars = createTemporaryObject(barsFactory, window.contentItem, {x: 20, y: 20, active: true})
        tryCompare(bars, "holding", true)
        window.hide()
        tryCompare(bars, "holding", false)
        compare(Pulse.held, 0)
        compare(findChild(bars, "playingBarsSurface").surface, null)
        window.show()
        tryCompare(bars, "holding", true)
        bars.active = false
    }

    function test_native_surface_follows_positioner_polish() {
        const row = createTemporaryObject(rowFactory, test)
        const bars = createTemporaryObject(barsFactory, row, {active: true})
        if (!bars.supportsShaders(GraphicsInfo.api)) skip("The renderer uses the rectangle fallback")
        const native = findChild(bars, "playingBarsSurface")
        tryVerify(() => native.surface !== null)
        tryCompare(native.surface, "x", row.x + 20 - bars.barScale)
        row.children[0].width = 60
        tryCompare(bars, "x", 60)
        tryCompare(native.surface, "x", row.x + 60 - bars.barScale)
    }

    function test_an_animated_scene_releases_the_native_surface() {
        const stage = createTemporaryObject(stageFactory, test, {width: 50, height: 30})
        const bars = createTemporaryObject(barsFactory, stage, {x: 4, y: 4, active: true})
        const native = findChild(bars, "playingBarsSurface")
        if (bars.supportsShaders(GraphicsInfo.api)) tryVerify(() => native.surface !== null)
        bars.sceneAnimating = true
        compare(native.active, false)
        compare(native.surface, null)
        verify(findChild(bars, "playingBarsRenderer").active)
        compare(Pulse.held, 1)
        bars.sceneAnimating = false
        if (bars.supportsShaders(GraphicsInfo.api)) tryVerify(() => native.surface !== null)
        bars.active = false
        compare(Pulse.held, 0)
    }

    function test_renderer_api_policy() {
        const bars = createTemporaryObject(barsFactory, test)
        for (const api of [GraphicsInfo.Unknown, GraphicsInfo.Null, GraphicsInfo.Software])
            compare(bars.supportsShaders(api), false)
        for (const api of [GraphicsInfo.OpenGL, GraphicsInfo.Vulkan, GraphicsInfo.Metal, GraphicsInfo.Direct3D11, GraphicsInfo.Direct3D12])
            compare(bars.supportsShaders(api), true)
        const renderer = findChild(bars, "playingBarsRenderer")
        compare(renderer.item.objectName, bars.supportsShaders(GraphicsInfo.api) ? "playingBarsShaderRenderer" : "playingBarsRectangles")
    }

    function test_rectangle_fallback_keeps_wave_geometry_and_single_pulse() {
        const bars = createTemporaryObject(barsFactory, test, {barScale: 1.5, tint: Qt.rgba(0.3, 0.6, 0.8, 0.5), shaderFailed: true})
        const renderer = findChild(bars, "playingBarsRenderer")
        compare(renderer.item.objectName, "playingBarsRectangles")
        compare(findChild(bars, "playingBarsShader"), null)
        compare(renderer.item.children.length, 4)
        const amplitudes = [0.6, 1, 0.4, 0.8], phases = [0, 0.25, 0.125, 0.375]
        for (const t of [0, 0.125, 0.25, 0.5, 0.875]) {
            bars.t = t
            for (let i = 0; i < 4; ++i) {
                const bar = renderer.item.children[i]
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

    function test_shader_phase_keeps_quad_geometry_fixed() {
        const stage = createTemporaryObject(stageFactory, test, {width: Metrics.eqWide * 1.5, height: Metrics.eqTall * 1.5})
        const bars = createTemporaryObject(barsFactory, stage, {barScale: 1.5, tint: "white"})
        if (!bars.supportsShaders(GraphicsInfo.api)) skip("The renderer uses the rectangle fallback")
        const effect = findChild(bars, "playingBarsShader")
        verify(effect !== null)
        compare(findChild(bars, "playingBarsRectangles"), null)
        // A cached QSB can render while Qt reports Uncompiled; I verify pixels.
        verify(waitForRendering(bars))
        // QtTest captures the window crop, so I provide an explicit black backdrop.
        const first = grabImage(stage)
        const sx = first.width / bars.width, sy = first.height / bars.height
        const amplitudes = [0.6, 1, 0.4, 0.8], phases = [0, 0.25, 0.125, 0.375]
        for (let i = 0; i < 4; ++i) {
            const x = (Metrics.eqStep * i + Metrics.eqBar / 2) * bars.barScale
            const h = bars.height * amplitudes[i] * bars.wave(phases[i])
            verify(first.red(Math.floor(x * sx), Math.floor((bars.height - h / 2) * sy)) > 128,
                "Every bar must render visible coverage")
            if (i < 3) {
                const gap = (Metrics.eqStep * i + (Metrics.eqBar + Metrics.eqStep) / 2) * bars.barScale
                verify(first.red(Math.floor(gap * sx), Math.floor((bars.height - bars.barScale) * sy)) < 32,
                    "The gaps between bars must show the black backdrop")
            }
        }
        bars.t = 0.5
        verify(waitForRendering(bars))
        const next = grabImage(stage)
        let changed = 0
        for (let y = 0; y < first.height; ++y)
            for (let x = 0; x < first.width; ++x)
                if (first.red(x, y) !== next.red(x, y)) ++changed
        verify(changed > 0, "A new phase must change the rendered bars")
        compare(bars.shaderFailed, false)
        verify(effect.status !== ShaderEffect.Error)
        const geometry = [effect.x, effect.y, effect.width, effect.height]
        compare(effect.width, bars.width + 2 * bars.barScale)
        compare(effect.height, bars.height + 2 * bars.barScale)
        for (const t of [0, 0.125, 0.25, 0.5, 0.875]) {
            bars.t = t
            compare(effect.phase, t)
            compare([effect.x, effect.y, effect.width, effect.height], geometry)
        }
    }

    function test_shader_error_switches_to_rectangles_without_stopping_motion() {
        const bars = createTemporaryObject(barsFactory, test)
        if (!bars.supportsShaders(GraphicsInfo.api)) skip("A shader renderer is required to report a shader error")
        bars.active = true
        compare(Pulse.held, 1)
        findChild(bars, "playingBarsShader").fragmentShader = Qt.resolvedUrl("missing-playing-bars.frag.qsb")
        tryCompare(bars, "shaderFailed", true)
        compare(findChild(bars, "playingBarsRenderer").item.objectName, "playingBarsRectangles")
        // The Loader releases its previous item through deferred deletion.
        tryVerify(() => findChild(bars, "playingBarsShader") === null)
        compare(Pulse.held, 1)
        const before = bars.t
        tryVerify(() => bars.t !== before, 500)
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
