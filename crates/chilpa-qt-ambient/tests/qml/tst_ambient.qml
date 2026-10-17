import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml/Theme"

// The dynamic background: which look mounts for which mode, when a track
// is needed, the veil by theme, the drift and the pulse it holds.
TestCase {
    id: test
    name: "Ambient"
    when: windowShown
    visible: true
    width: 640
    height: 400

    Component { id: maker; AppBackground { anchors.fill: parent; plane: Qt.rect(100, 40, 400, 360) } }
    property var made: null

    function mount(mode, path) {
        Ambient.mode = mode
        Ambient.on = false
        Player.path = path
        made = maker.createObject(test)
        waitForRendering(made)
    }
    function cleanup() {
        if (made)
            made.destroy()
        made = null
        Pulse.held = 0
        Pulse.ms = 0
        ThemeModel.dark = true
        Player.art = ""
        Ambient.followed = []
        Ambient.demands = ({})
        Ambient.refreshes = 0
        Ambient.reloads = 0
    }
    function field(type) {
        for (let i = 0; i < made.children.length; ++i)
            if (made.children[i].toString().indexOf(type) === 0)
                return made.children[i]
        return null
    }
    function fields(type) {
        const found = []
        for (let i = 0; i < made.children.length; ++i)
            if (made.children[i].toString().indexOf(type) === 0)
                found.push(made.children[i])
        return found
    }

    function test_off_mounts_nothing_and_says_so() {
        mount(0, "/a.flac")
        compare(made.on, false)
        compare(Ambient.on, false)
        verify(!field("AmbientField").visible)
        verify(!field("WallpaperField").visible)
    }

    function test_the_field_waits_for_a_track() {
        mount(1, "")
        compare(made.on, false, "no track, no field")
        Player.path = "/a.flac"
        compare(made.on, true)
        compare(Ambient.on, true, "the theme is told")
        verify(field("AmbientField").visible)
        verify(field("AmbientField").running)
        compare(Ambient.followed.length, 1, "I request colors when the field gains a track")
    }

    function test_off_and_blurred_backgrounds_do_not_extract_cover_colors() {
        mount(0, "/a.flac")
        Player.art = "file:///first.png"
        compare(Ambient.followed.length, 0)
        Ambient.mode = 2
        Player.art = "file:///second.png"
        compare(Ambient.followed.length, 0)
        Ambient.mode = 1
        compare(Ambient.followed.length, 1)
        compare(Ambient.demands.main, "file:///second.png")
        made.visible = false
        compare(Ambient.demands.main, undefined)
    }

    Component { id: layerMaker; AtmosphereLayers { width: 640; height: 400; shown: false; fallbackSource: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png" } }
    function test_hidden_atmosphere_releases_its_fallback_image() {
        const layers = createTemporaryObject(layerMaker, test)
        const fallback = layers.children.find(item => item.sourceSize !== undefined)
        verify(fallback !== undefined)
        compare(fallback.source.toString(), "")
        layers.shown = true
        verify(fallback.source.toString() !== "")
        verify(fallback.sourceSize.width <= layers.width + Field.staticGrow)
        layers.shown = false
        compare(fallback.source.toString(), "", "I release the decoded fallback when hidden")
    }

    Component { id: miniColors; CoverColors { consumer: "mini"; active: true; art: "file:///mini.png" } }
    function test_the_mini_keeps_its_colors_with_the_main_background_off() {
        mount(0, "/a.flac")
        const mini = createTemporaryObject(miniColors, test)
        compare(Ambient.demands.main, undefined)
        compare(Ambient.demands.mini, "file:///mini.png")
        mini.active = false
        compare(Ambient.demands.mini, undefined)
    }

    function test_the_wallpaper_modes_do_not_wait() {
        mount(3, "")
        compare(made.on, true)
        verify(field("WallpaperField").visible)
        verify(Ambient.refreshes >= 1, "the wallpaper was resolved on mount")
        Ambient.mode = 4
        verify(!field("WallpaperField").visible)
        const atmospheres = fields("AtmosphereField")
        compare(atmospheres.length, 2)
        verify(atmospheres[1].visible, "the second atmosphere is the wallpaper's")
        verify(!atmospheres[0].visible)
    }

    function test_the_veil_is_dark_at_the_dim_or_light_a_notch_stronger() {
        mount(1, "/a.flac")
        compare(made.veilStrength, Metrics.ambientDim)
        compare(made.innerDim, Metrics.ambientDim)
        ThemeModel.dark = false
        compare(made.veilStrength, Metrics.ambientVeilLight)
        compare(made.innerDim, 0, "a light theme does not darken inside the field")
    }

    function test_the_field_holds_the_pulse_while_it_moves_and_lets_go() {
        mount(1, "/a.flac")
        compare(Pulse.held, 1)
        made.visible = false
        compare(Pulse.held, 0)
        made.visible = true
        compare(Pulse.held, 1)
        Player.path = ""
        compare(Pulse.held, 0, "no track, no field, no pulse")
    }

    function test_the_atmosphere_drifts_on_the_pulse_and_rests_at_zero() {
        mount(2, "/a.flac")
        const atmosphere = fields("AtmosphereField")[0]
        Player.playing = true
        Player.paused = false
        compare(Pulse.held, 1)
        Pulse.ms += 33
        Pulse.ms += 33
        compare(atmosphere.tick, 2 * Field.atmosphereTick)
        Player.paused = true
        compare(atmosphere.tick, 0, "the still pose is always the same one")
        compare(Pulse.held, 0)
        Player.paused = false
    }

    function test_a_kept_preference_is_read_again() {
        mount(1, "/a.flac")
        Prefs.changed()
        compare(Ambient.reloads, 1)
    }

    function test_the_blur_follows_the_slider_before_it_is_kept() {
        mount(3, "")
        compare(made.blur, 0.75)
        Prefs.preview("appearance.wallpaperBlur", "20")
        compare(made.blur, 0.2)
        Prefs.preview("", "")
        compare(made.blur, 0.75)
    }

    Component { id: wallpaperMaker; WallpaperField { width: 300; height: 200; isWayland: true } }

    function test_the_compositor_rectangle_is_the_one_our_size() {
        const field = wallpaperMaker.createObject(test)
        const rects = [{ x: 0, y: 0, w: 500, h: 500 }, { x: 640, y: 120, w: 310, h: 190 }]
        compare(field.pickRect(rects, 300, 200).x, 640)
        compare(field.pickRect(rects, 1000, 900), null, "nothing within tolerance is nothing")
        compare(field.pickRect([{ x: 1 }], 300, 200), null, "a rectangle without a size is skipped")
        field.destroy()
    }
}
