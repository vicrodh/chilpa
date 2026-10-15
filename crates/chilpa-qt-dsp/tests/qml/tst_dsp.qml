import QtQuick
import QtTest
import app.chilpa.dsp
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
