import QtQuick
import QtTest
import app.chilpa.dsp
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

TestCase {
    name: "DspControls"
    when: windowShown
    visible: true
    width: 900
    height: 850
    DspControl { id: control }
    DspPanel { id: panel }
    DspQualityNote { id: quality; width: 400; y: 750 }
    function init() {
        Dsp.actions = []
        Dsp.available = true
        Dsp.active = false
        Dsp.presetName = "Sennheiser HD650"
        Dsp.snapshot = JSON.stringify({available: true, selection: {plugin: "test.dsp", preset: "hd650", enabled: false}, preset: {id: "hd650", name: "Sennheiser HD650", description: "Measured profile", filters: []},
            presets: [{plugin: "test.dsp", preset: {id: "hd650", name: "Sennheiser HD650", category: "Headphones", description: "Measured profile"}},
                {plugin: "test.dsp", preset: {id: "soundbar", name: "Soundbar 2.1", category: "Speakers", description: "Generic"}}], favorites: ["test.dsp/hd650"], curve: [0, 1, 0]})
        panel.group = "all"
        quality.casting = false
    }
    function cleanup() { panel.close() }
    function graphicState() {
        const state = JSON.parse(Dsp.snapshot)
        state.graphic = true
        state.preset.filters = [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000].map(
            frequency => ({shape: "peak", frequency: frequency, gain_db: 0, q: Math.SQRT2}))
        state.preset.preamp_db = -4
        state.preset.balance = 0.2
        Dsp.snapshot = JSON.stringify(state)
        return state
    }
    function lastAction() { return Dsp.actions[Dsp.actions.length - 1] }
    function visualNamed(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = visualNamed(child, name)
            if (found) return found
        }
        return null
    }
    function test_graphic_slider_keyboard_edits_real_filter_and_keeps_other_parameters() {
        graphicState()
        panel.open()
        const slider = visualNamed(panel.contentItem, "dspGraphicBand3")
        verify(slider.visible)
        slider.forceActiveFocus()
        keyClick(Qt.Key_Up)
        compare(lastAction().action, "edit")
        compare(lastAction().input.filters[3].frequency, 250)
        compare(lastAction().input.filters[3].gain_db, 0.5)
        compare(lastAction().input.balance, 0.2)
        compare(lastAction().input.preamp_db, -4)
        for (let index = 0; index < 10; ++index) {
            if (index !== 3) compare(lastAction().input.filters[index].gain_db, 0)
        }
    }
    function test_graphic_drag_commits_only_when_released() {
        graphicState()
        panel.open()
        const slider = visualNamed(panel.contentItem, "dspGraphicBand5")
        const previous = Dsp.actions.length
        mousePress(slider, slider.width / 2, slider.height / 2)
        mouseMove(slider, slider.width / 2, slider.height / 4)
        compare(Dsp.actions.length, previous)
        mouseRelease(slider, slider.width / 2, slider.height / 4)
        compare(Dsp.actions.length, previous + 1)
        compare(lastAction().action, "edit")
        verify(lastAction().input.filters[5].gain_db > 0)
    }
    function test_a_band_shows_its_value_beside_the_handle_while_it_is_moved() {
        const state = graphicState()
        state.preset.filters[4].gain_db = 3.5
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        const slider = visualNamed(panel.contentItem, "dspGraphicBand4")
        const tip = visualNamed(panel.contentItem, "dspGraphicValue4")
        verify(!tip.visible, "nothing beside it until it is used")
        mouseMove(panel.contentItem, 0, 0)
        mousePress(slider, slider.width / 2, slider.handle.y + slider.handle.height / 2)
        verify(tip.visible)
        verify(visualNamed(tip, "dspGraphicValueText4") !== null)
        compare(visualNamed(tip, "dspGraphicValueText4").text, "+3.5 dB")
        verify(tip.x + tip.width <= slider.handle.x, "to the left of the handle")
        mouseRelease(slider, slider.width / 2, slider.handle.y + slider.handle.height / 2)
        const first = visualNamed(panel.contentItem, "dspGraphicBand0")
        first.forceActiveFocus(Qt.TabFocusReason)
        const firstTip = visualNamed(panel.contentItem, "dspGraphicValue0")
        verify(firstTip.visible, "the keys show it too")
        verify(firstTip.x >= first.handle.x + first.handle.width, "the first band has no room on its left")
    }
    function test_graphic_reset_zeroes_bands_without_changing_preamp_or_stereo() {
        const state = graphicState()
        state.preset.filters[0].gain_db = 6
        state.preset.filters[8].gain_db = -3
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        findChild(panel, "dspResetGraphic").clicked()
        compare(lastAction().action, "edit")
        verify(lastAction().input.filters.every(filter => filter.gain_db === 0))
        compare(lastAction().input.preamp_db, -4)
        compare(lastAction().input.balance, 0.2)
    }
    function test_a_precise_preset_shows_its_levels_and_converts_only_on_request() {
        const state = JSON.parse(Dsp.snapshot)
        state.preset.filters = [{shape: "low_shelf", frequency: 90, gain_db: 3, q: 0.7}]
        state.levels = [3, 2.5, 1.5, 0.5, 0, 0, 0, 0, 0, -1]
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        compare(Dsp.actions.length, 0)
        const band = visualNamed(panel.contentItem, "dspGraphicBand0")
        verify(band.visible, "the preset's shape shows without a click")
        compare(band.value, 3)
        compare(visualNamed(panel.contentItem, "dspGraphicBand9").value, -1)
        verify(!band.enabled, "its bands are read, not moved")
        band.forceActiveFocus()
        keyClick(Qt.Key_Up)
        compare(Dsp.actions.length, 0)
        verify(!findChild(panel, "dspResetGraphic").visible)
        const start = findChild(panel, "dspStartGraphic")
        verify(start.visible)
        start.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(lastAction().action, "graphic_start")
    }
    function test_restore_and_preamp_controls_send_real_commands() {
        const state = graphicState()
        state.selection.graphic_origin = {preset: "hd650", edited: null, enabled: false}
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        const restore = findChild(panel, "dspRestorePrevious")
        verify(restore.visible)
        restore.forceActiveFocus()
        keyClick(Qt.Key_Space)
        compare(lastAction().action, "graphic_restore")
        const preamp = findChild(panel, "dspPreamp")
        preamp.forceActiveFocus()
        keyClick(Qt.Key_Left)
        compare(lastAction().action, "edit")
        compare(lastAction().input.preamp_db, -4.5)
        compare(lastAction().input.filters.length, 10)
    }
    function test_icon_follows_plugin_availability() {
        verify(control.visible)
        Dsp.available = false
        verify(!control.visible)
    }
    function test_quality_note_names_only_the_active_local_preset() {
        verify(!quality.visible)
        Dsp.active = true
        verify(quality.visible)
        verify(quality.text.includes("Sennheiser HD650"))
        verify(quality.text.includes("bit-perfect"))
        quality.casting = true
        verify(!quality.visible)
        quality.casting = false
        Dsp.active = false
        verify(!quality.visible)
    }
    function test_groups_search_and_favorites_reduce_the_catalog() {
        panel.open()
        compare(panel.shown.length, 2)
        panel.group = "Headphones"
        compare(panel.shown.length, 1)
        compare(panel.shown[0].preset.id, "hd650")
        panel.group = "favorites"
        compare(panel.shown.length, 1)
        panel.group = "all"
        const search = findChild(panel, "dspSearch")
        search.text = "soundbar"
        compare(panel.shown.length, 1)
        compare(panel.shown[0].preset.id, "soundbar")
        search.text = ""
    }
    function test_disclaimer_and_favorite_actions_are_visible() {
        panel.open()
        verify(findChild(panel, "dspDisclaimer").text.includes("bit-perfect"))
        const favorite = findChild(panel, "dspFavorite")
        verify(favorite.checked)
        favorite.clicked()
        compare(Dsp.actions[Dsp.actions.length - 1].action, "favorite")
        compare(Dsp.actions[Dsp.actions.length - 1].input.key, "test.dsp/hd650")
        findChild(panel, "dspEnabled").toggled(true)
        compare(Dsp.actions[Dsp.actions.length - 1].input.enabled, true)
    }
}
