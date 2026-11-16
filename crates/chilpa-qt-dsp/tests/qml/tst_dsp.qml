import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.dsp
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

TestCase {
    id: suite
    name: "DspControls"
    when: windowShown
    visible: true
    width: 1440
    height: 1000
    DspControl { id: control }
    DspPanel { id: panel }
    DspEditQueue { id: editQueue }
    SignalSpy { id: queuedCommands; target: editQueue; signalName: "command" }
    DspQualityNote { id: quality; width: 400; y: 750 }
    function init() {
        suite.Window.window.width = 1440
        suite.Window.window.height = 1000
        Dsp.actions = []
        editQueue.cancel()
        queuedCommands.clear()
        Dsp.available = true
        Dsp.active = false
        Dsp.presetName = "Sennheiser HD650"
        Dsp.snapshot = JSON.stringify({available: true, selection: {plugin: "test.dsp", preset: "hd650", enabled: false}, preset: {id: "hd650", name: "Sennheiser HD650", description: "Measured profile", filters: []},
            graphic_frequencies: [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000],
            browser: {presets: [], groups: [{key: "all", label: "All"}, {key: "Headphones", label: "Headphones"}, {key: "favorites", label: "Favorites"}], total: 2, next: null, selected_ids: []},
            presets: [{plugin: "test.dsp", preset: {id: "hd650", name: "Sennheiser HD650", category: "Headphones", description: "Measured profile"}},
                {plugin: "test.dsp", preset: {id: "soundbar", name: "Soundbar 2.1", category: "Speakers", description: "Generic"}}], favorites: ["test.dsp/hd650"], curve: [0, 1, 0]})
        panel.group = "all"
        panel.query = ""
        quality.casting = false
    }
    function cleanup() {
        panel.close()
        const dialog = findChild(control, "dspDialogLoader")
        if (dialog.item) dialog.item.close()
    }
    function processingActions() { return Dsp.actions.filter(entry => !["search", "close", "select"].includes(entry.action)) }
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
    function test_paged_selected_profile_remains_named_in_the_quick_picker() {
        const state = graphicState()
        state.selection.preset = "measured-variant"
        state.preset.id = "measured-variant"
        state.preset.name = "Measured headphone variant"
        state.preset.category = "Headphones"
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        wait(0)
        const picker = visualNamed(panel.contentItem, "dspPresetPicker")
        verify(picker.current >= 0)
        compare(picker.options[picker.current], state.preset.name)
        const options = picker.options
        state.meters = [0.1, 0.1, 0, 0]
        Dsp.snapshot = JSON.stringify(state)
        wait(0)
        compare(picker.options, options)
    }
    function test_zero_compressor_threshold_stays_zero_and_can_be_lowered() {
        const state = graphicState()
        state.preset.compressor_threshold_db = 0
        state.preset.compressor_ratio = 1
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        panel.section = "dynamics"
        wait(0)
        const slider = visualNamed(panel.contentItem, "dspParameter_compressor_threshold_db")
        compare(slider.parent.parent.value, 0)
        slider.forceActiveFocus()
        keyClick(Qt.Key_Left)
        compare(lastAction().input.compressor_threshold_db, -0.5)
    }
    function test_graphic_slider_keyboard_dispatches_a_bounded_band_edit() {
        graphicState()
        panel.open()
        wait(0)
        const slider = visualNamed(panel.contentItem, "dspGraphicBand3")
        verify(slider.visible)
        slider.forceActiveFocus()
        keyClick(Qt.Key_Up)
        compare(lastAction().action, "graphic_band")
        compare(lastAction().input.index, 3)
        compare(lastAction().input.gain, 0.5)
        compare(JSON.parse(Dsp.snapshot).preset.balance, 0.2)
        compare(JSON.parse(Dsp.snapshot).preset.preamp_db, -4)
    }
    function test_graphic_drag_auditions_at_a_bounded_rate_and_commits_on_release() {
        graphicState()
        panel.open()
        wait(0)
        const slider = visualNamed(panel.contentItem, "dspGraphicBand5")
        const previous = Dsp.actions.length
        mousePress(slider, slider.width / 2, slider.height / 2)
        mouseMove(slider, slider.width / 2, slider.height / 4)
        wait(Metrics.dspPreviewInterval * 2)
        verify(Dsp.actions.length > previous)
        compare(lastAction().action, "graphic_preview")
        verify(Dsp.actions.length <= previous + 3)
        mouseRelease(slider, slider.width / 2, slider.height / 4)
        compare(lastAction().action, "graphic_band")
        compare(lastAction().input.index, 5)
        verify(lastAction().input.gain > 0)
    }
    function test_a_band_shows_its_value_beside_the_handle_while_it_is_moved() {
        const state = graphicState()
        state.preset.filters[4].gain_db = 3.5
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        wait(0)
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
        wait(0)
        findChild(panel, "dspResetGraphic").clicked()
        compare(lastAction().action, "graphic_reset")
        compare(JSON.parse(Dsp.snapshot).preset.preamp_db, -4)
        compare(JSON.parse(Dsp.snapshot).preset.balance, 0.2)
    }
    function test_a_precise_preset_shows_its_levels_and_converts_only_on_request() {
        const state = JSON.parse(Dsp.snapshot)
        state.preset.filters = [{shape: "low_shelf", frequency: 90, gain_db: 3, q: 0.7}]
        state.levels = [3, 2.5, 1.5, 0.5, 0, 0, 0, 0, 0, -1]
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        wait(0)
        panel.section = "graphic"
        wait(0)
        compare(processingActions().length, 0)
        const band = visualNamed(panel.contentItem, "dspGraphicBand0")
        verify(band.visible, "the preset's shape shows without a click")
        compare(band.value, 3)
        compare(visualNamed(panel.contentItem, "dspGraphicBand9").value, -1)
        verify(!band.enabled, "its bands are read, not moved")
        band.forceActiveFocus()
        keyClick(Qt.Key_Up)
        compare(processingActions().length, 0)
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
        wait(0)
        const restore = findChild(panel, "dspRestorePrevious")
        verify(restore.visible)
        restore.forceActiveFocus()
        keyClick(Qt.Key_Space)
        compare(lastAction().action, "graphic_restore")
        panel.section = "headroom"
        wait(0)
        const preamp = findChild(panel, "dspPreamp")
        compare(preamp.value, -4)
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
    function test_player_creates_the_dialog_only_on_request_and_releases_it_after_close() {
        const dialog = findChild(control, "dspDialogLoader")
        compare(dialog.item, null)
        control.clicked()
        wait(0)
        verify(dialog.item !== null)
        verify(dialog.item.opened)
        dialog.item.close()
        tryCompare(dialog, "item", null)
        control.clicked()
        wait(0)
        verify(dialog.item !== null)
        verify(dialog.item.opened)
        compare(processingActions().length, 0)
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
    function test_groups_and_search_dispatch_queries_and_render_the_returned_page() {
        panel.open()
        wait(0)
        panel.section = "presets"
        compare(lastAction().action, "search")
        panel.group = "Headphones"
        compare(lastAction().input.group, "Headphones")
        panel.group = "favorites"
        compare(lastAction().input.group, "favorites")
        panel.group = "all"
        const search = findChild(panel, "dspSearch")
        search.text = "soundbar"
        wait(Metrics.dspSearchInterval * 2)
        compare(lastAction().action, "search")
        compare(lastAction().input.query, "soundbar")
        const state = JSON.parse(Dsp.snapshot)
        state.browser.presets = [state.presets[1]]
        state.browser.total = 1
        Dsp.snapshot = JSON.stringify(state)
        compare(panel.shown.length, 1)
        compare(panel.shown[0].preset.id, "soundbar")
        search.text = ""
    }
    function test_sidebar_changes_sections_without_processing_audio() {
        panel.open()
        wait(0)
        verify(panel.wide)
        verify(findChild(panel.contentItem, "dspResponseCurve") !== null)
        const headroom = findChild(panel.contentItem, "dspSection_headroom")
        headroom.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(panel.section, "headroom")
        verify(findChild(panel.contentItem, "dspPreamp") !== null)
        verify(findChild(panel.contentItem, "dspResponseCurve") === null)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Down)
        compare(panel.section, "dynamics")
        compare(processingActions().length, 0)
    }
    function test_import_text_survives_switching_sections() {
        panel.open()
        wait(0)
        panel.section = "import"
        wait(0)
        const profile = findChild(panel.contentItem, "dspProfileText")
        profile.forceActiveFocus()
        profile.text = "Preamp: -6 dB"
        compare(panel.importText, profile.text)
        panel.section = "headroom"
        wait(0)
        verify(findChild(panel.contentItem, "dspImportEditor") === null)
        panel.section = "import"
        wait(0)
        compare(findChild(panel.contentItem, "dspProfileText").text, "Preamp: -6 dB")
        compare(processingActions().length, 0)
    }
    function test_workspace_fits_resized_windows_and_keeps_footer_reachable() {
        panel.open()
        wait(0)
        panel.section = "presets"
        wait(0)
        for (const size of [[1440, 1000], [960, 600], [640, 480]]) {
            suite.Window.window.width = size[0]
            suite.Window.window.height = size[1]
            wait(0)
            verify(panel.width <= size[0] - Metrics.modalInsets)
            verify(panel.height <= size[1])
            const footer = findChild(panel.contentItem, "dspDisclaimer")
            const position = footer.mapToItem(panel.contentItem, 0, 0)
            verify(position.y >= 0)
            verify(position.y + footer.height <= panel.contentItem.height)
            verify(findChild(panel.contentItem, "dspPageLoader").height > 0)
            compare(findChild(panel.contentItem, "dspCompactNavigation").visible, !panel.wide)
            panel.section = "parametric"
            wait(0)
            const curve = findChild(panel.contentItem, "dspResponseCurve")
            verify(curve.height <= findChild(panel.contentItem, "dspPageLoader").height * Metrics.dspCurveHeightShare)
            panel.section = "presets"
            wait(0)
        }
    }
    function test_closing_releases_the_active_editor_and_reopening_restores_it() {
        panel.open()
        wait(0)
        const loader = findChild(panel.contentItem, "dspPageLoader")
        verify(loader.item !== null)
        panel.close()
        tryCompare(loader, "item", null)
        panel.open()
        wait(0)
        verify(loader.item !== null)
        compare(panel.section, "parametric")
        compare(processingActions().length, 0)
    }
    function test_disclaimer_and_favorite_actions_are_visible() {
        panel.open()
        wait(0)
        verify(findChild(panel, "dspDisclaimer").text.includes("bit-perfect"))
        const favorite = findChild(panel, "dspFavorite")
        verify(favorite.checked)
        favorite.clicked()
        compare(Dsp.actions[Dsp.actions.length - 1].action, "favorite")
        compare(Dsp.actions[Dsp.actions.length - 1].input.key, "test.dsp/hd650")
        findChild(panel, "dspEnabled").toggled(true)
        compare(Dsp.actions[Dsp.actions.length - 1].input.enabled, true)
    }
    function test_tone_knob_keyboard_keeps_the_correction_filters() {
        const state = graphicState()
        state.preset.tone = {enabled: true, bass_db: 4, bass_hz: 90, bass_q: 0.8, treble_db: 2, treble_hz: 10000, treble_q: 0.6}
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        panel.section = "tone"
        wait(0)
        const bass = findChild(panel.contentItem, "dspKnob_bass")
        verify(bass !== null)
        bass.forceActiveFocus()
        keyClick(Qt.Key_Right)
        compare(lastAction().action, "edit")
        compare(lastAction().input.tone.bass_db, 4.5)
        compare(lastAction().input.filters.length, 10)
        compare(lastAction().input.filters[3].frequency, 250)
    }
    function test_parametric_filters_are_virtualized_and_reach_the_last_band() {
        const state = JSON.parse(Dsp.snapshot)
        state.preset.filters = Array.from({length: 64}, (_, index) => ({shape: "peak", frequency: 100 + index * 100, gain_db: 0, q: 1, enabled: true, channel: "all", locked: false}))
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        wait(0)
        const list = findChild(panel.contentItem, "dspFilterList")
        compare(list.count, 64)
        verify(list.contentItem.children.length < 16)
        list.positionViewAtEnd()
        wait(0)
        verify(list.itemAtIndex(63) !== null)
        const position = list.contentY
        state.preset.filters[63].q = 2
        Dsp.snapshot = JSON.stringify(state)
        wait(0)
        verify(Math.abs(list.contentY - position) < 1, "editing the last filter must preserve scroll")
    }
    function test_personal_management_uses_real_save_rename_and_autosave_actions() {
        panel.open()
        panel.section = "manage"
        wait(0)
        const name = findChild(panel.contentItem, "dspSaveName")
        name.text = "My correction"
        findChild(panel.contentItem, "dspSavePreset").clicked()
        compare(lastAction().action, "save")
        compare(lastAction().input.name, "My correction")
        const state = JSON.parse(Dsp.snapshot)
        state.selection.preset = "user-1"
        state.selection.edited = state.preset
        Dsp.snapshot = JSON.stringify(state)
        findChild(panel.contentItem, "dspRenamePreset").clicked()
        compare(lastAction().action, "rename")
        findChild(panel.contentItem, "dspReplacePreset").clicked()
        compare(lastAction().action, "replace")
    }

    function test_curve_drag_and_wheel_dispatch_parameters_and_respect_locks() {
        const state = JSON.parse(Dsp.snapshot)
        state.preset.filters = [{shape: "peak", frequency: 1000, gain_db: 3, q: 1, enabled: true, channel: "all", locked: false}]
        Dsp.snapshot = JSON.stringify(state)
        panel.open()
        wait(0)
        const plot = findChild(panel.contentItem, "dspResponseCurve")
        const chart = plot.parent
        const x = chart.xFor(1000), y = chart.yFor(3)
        mousePress(plot, x, y)
        mouseMove(plot, x + 24, y - 12)
        wait(Metrics.dspPreviewInterval * 2)
        compare(lastAction().action, "filter_preview")
        compare(lastAction().input.index, 0)
        verify(lastAction().input.frequency > 1000)
        mouseRelease(plot, x + 24, y - 12)
        compare(lastAction().action, "filter_commit")
        verify(lastAction().input.gain_db > 3)
        mouseWheel(plot, x, y, 0, 120)
        compare(lastAction().action, "filter_edit")
        compare(lastAction().input.field, "q")
        verify(lastAction().input.value > 1)
        state.preset.filters[0].locked = true
        Dsp.snapshot = JSON.stringify(state)
        wait(0)
        const previous = Dsp.actions.length
        mousePress(plot, x, y); mouseMove(plot, x + 20, y - 10); mouseRelease(plot, x + 20, y - 10)
        mouseWheel(plot, x, y, 0, 120)
        wait(Metrics.dspPreviewInterval * 2)
        compare(Dsp.actions.length, previous)
    }
    function test_preview_queue_groups_updates_and_flushes_the_final_value() {
        editQueue.preview("preview", {value: 1})
        editQueue.preview("preview", {value: 2})
        editQueue.preview("preview", {value: 3})
        wait(Metrics.dspPreviewInterval * 2)
        compare(queuedCommands.count, 1)
        compare(queuedCommands.signalArguments[0][1].value, 3)
        editQueue.flush()
        compare(queuedCommands.count, 2)
        compare(queuedCommands.signalArguments[1][0], "edit")
        compare(queuedCommands.signalArguments[1][1].value, 3)
        editQueue.preview("preview", {value: 4})
        editQueue.commit("edit", {value: 5})
        wait(Metrics.dspPreviewInterval * 2)
        compare(queuedCommands.count, 3)
        compare(queuedCommands.signalArguments[2][1].value, 5)
    }

}
