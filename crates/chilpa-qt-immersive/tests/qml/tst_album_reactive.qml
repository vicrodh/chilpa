import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive
import "../../qml/Patterns"

TestCase {
    id: test
    name: "AlbumReactive"
    when: windowShown
    visible: true
    width: 1100
    height: 1024
    AlbumReactivePanel { id: panel; width: test.width; height: 1024; shaders: false }

    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) {
            const result = find(child, predicate)
            if (result) return result
        }
        return null
    }
    function pulse(level) {
        Visualizer.energy = new Array(5).fill(level)
        Pulse.ms += Pulse.period
        wait(0)
    }
    function init() {
        Immersive.open = true
        panel.height = 1024
        Player.art = "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024"><rect width="1024" height="1024" fill="#d64424"/></svg>')
        for (let i = 0; i < 20; ++i) pulse(0)
        tryVerify(() => panel.ownWidth === 1024)
    }
    function cleanup() { Immersive.open = false }

    function test_audio_pulses_keep_the_decoded_cover_and_geometry() {
        const tile = find(panel, item => item.decode !== undefined && item.ready !== undefined)
        const image = find(tile, item => item.sourceSize !== undefined)
        tryVerify(() => tile.ready)
        const before = {width:tile.width, height:tile.height, decode:tile.decode,
            source:image.source.toString(), size:image.sourceSize.width}
        compare(panel.baseArt, 512, "the resting cover touches a decode bucket boundary")
        for (const level of [1, 0, 1, 0, 1]) {
            pulse(level)
            compare(tile.width, before.width, "music transforms the loaded cover instead of resizing its decode")
            compare(tile.height, before.height)
            compare(tile.decode, before.decode)
            compare(image.source.toString(), before.source)
            compare(image.sourceSize.width, before.size)
            verify(tile.ready)
        }
        verify(panel.artScale > 1, "the stable texture still breathes with the music")
    }
    function test_window_resize_still_adapts_the_cover() {
        const before = panel.baseArt
        panel.height = 900
        verify(panel.baseArt < before)
        pulse(1)
        verify(panel.artScale > 1)
    }
}
