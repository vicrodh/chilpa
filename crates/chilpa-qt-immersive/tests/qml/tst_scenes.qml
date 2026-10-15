import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

TestCase {
    id: test
    name: "ImmersiveScenes"
    when: windowShown
    visible: true
    width: 900
    height: 600
    ShaderSceneLayer { id: layer; anchors.fill: parent }
    ViewMenu { id: menu }

    function init() {
        Motion.reduced = false
        Player.playing = true
        Player.paused = false
        Immersive.accelerated = true
        Immersive.open = true
        Immersive.scene = 1
        layer.visible = true
        layer.shaderFailed = false
        layer.timeS = 0
    }
    function cleanup() {
        Immersive.scene = 0
        Immersive.open = false
        Motion.reduced = false
    }
    function test_only_the_current_scene_exists() {
        for (const scene of [1, 4, 5, 8]) {
            Immersive.scene = scene
            wait(1)
            const loaders = layer.children.filter(item => item.item !== undefined)
            compare(loaders.filter(item => item.item !== null).length, 1)
        }
        Immersive.open = false
        wait(1)
        const loaders = layer.children.filter(item => item.item !== undefined)
        compare(loaders.filter(item => item.item !== null).length, 0)
    }
    function test_pause_hidden_and_reduced_motion_release_the_clock() {
        verify(layer.moving)
        verify(layer.holding)
        Player.paused = true
        verify(!layer.moving)
        verify(!layer.holding)
        const frozen = layer.timeS
        Pulse.ms += Pulse.period
        compare(layer.timeS, frozen)
        Player.paused = false
        Motion.reduced = true
        verify(!layer.holding)
        Motion.reduced = false
        layer.visible = false
        verify(!layer.holding)
        Pulse.ms += Pulse.period
        compare(layer.timeS, frozen)
    }
    function test_a_paused_bed_restores_its_last_frame_after_hiding() {
        Immersive.scene = 5
        const samples = new Float32Array(256 * 200)
        samples[0] = 42
        Visualizer.linebedHeights = samples.buffer
        Pulse.ms += Pulse.period
        compare(layer.pendingHeights, null)
        Player.paused = true
        layer.visible = false
        wait(1)
        layer.visible = true
        wait(1)
        const loaders = layer.children.filter(item => item.item !== undefined && item.item !== null)
        compare(loaders.length, 1)
        compare(new Float32Array(loaders[0].item.heights)[0], 42)
        verify(!layer.holding)
    }
    function test_audio_pack_applies_on_the_shared_pulse() {
        Visualizer.packJson = JSON.stringify({phase: 4, beat: 0.7, beatAbove: 0.4, level: 0.5,
            levelSmooth: 0.3, transient: 0.8, energyLo: [0.1, 0.2, 0.3, 0.4],
            energyHi: [0.5, 0.6, 0, 0], bandsLo: [0.9, 0.8, 0.7, 0.6], bandsHi: [0.5, 0.4, 0.3, 0.2]})
        Pulse.ms += Pulse.period
        compare(layer.beatAbove, 0.4)
        compare(layer.phase, 4)
        compare(layer.energyHi.y, 0.6)
        compare(layer.bandsLo.x, 0.9)
    }
    function test_picker_has_all_seven_scenes_and_removes_them_without_gpu() {
        compare(menu.focusRows.filter(row => row.scene !== undefined).length, 7)
        menu.fireRow(menu.focusRows.find(row => row.scene === 8))
        compare(Immersive.scene, 8)
        compare(Immersive.viewMode, 0)
        menu.fireRow(menu.focusRows.find(row => row.mode === 3))
        compare(Immersive.scene, 0)
        compare(Immersive.mode, 3)
        Immersive.accelerated = false
        compare(menu.focusRows.filter(row => row.scene !== undefined).length, 0)
        compare(menu.focusRows.filter(row => row.mode === 7 || row.mode === 8).length, 0)
    }
}
