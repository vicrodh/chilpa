import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/dsp/qml/Components" as Parts

Window {
    id: window
    visible: true
    width: 1440
    height: 1000
    color: Theme.color.surfaceWindow
    Component.onCompleted: Motion.reduced = true
    property string pluginId: "PLUGIN_ID"
    property string mode: "SMOKE_MODE"
    property string captureDirectory: "CAPTURE_DIR"
    property int step: 0
    property int captures: 0
    property bool stopped: false
    property string originalFilters: ""
    property string savedId: ""
    property string remoteId: ""
    property string savedName: "DSP smoke final preset"
    readonly property bool playing: Player.playing
    Parts.DspControl { id: control }
    Parts.DspPanel { id: panel }
    function check(condition, message) { if (!condition) throw new Error(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) { const found = find(child, predicate); if (found) return found }
        return null
    }
    function named(name) {
        const item = find(panel.contentItem, item => item.objectName === name)
        check(!!item, "missing real editor control: " + name)
        return item
    }
    function setSlider(name, value) { const slider = named(name); slider.value = value; slider.moved() }
    function capture(name, after) {
        ++captures
        const accepted = window.contentItem.grabToImage(result => {
            check(result.saveToFile(captureDirectory + "/" + name + ".png"), "could not save native UI capture")
            --captures
            if (after) after()
        })
        check(accepted, "native UI capture was rejected")
    }
    function browserReady(state, query, group) { return !state.browser.loading && state.browser.query === query && state.browser.group === group }
    function complete(state) {
        if (captures > 0) return
        stopped = true
        console.log("SMOKE DSP snapshot " + JSON.stringify(state))
        console.log("SMOKE DSP " + mode + " completed: presets=" + state.presets.length + ", active=false")
        panel.close()
        Qt.quit()
    }
    function checkSaved(state) {
        check(state.preset.name === savedName && state.selection.preset === savedId && !state.selection.edited, "personal identity or committed settings changed")
        check(state.preset.filters.length === 1 && state.preset.filters[0].frequency === 110 && state.preset.filters[0].gain_db === 2.5, "imported correction changed")
        check(state.preset.preamp_db === -7 && state.preset.tone.bass_db === 4 && state.preset.tone.treble_db === 2, "tone or preamp did not persist")
        check(state.preset.dynamics.attack_ms === 17 && state.preset.dynamics.knee_db === 6, "advanced compressor did not persist")
        check(state.preset.limiter.enabled && state.preset.limiter.ceiling_db === -2, "independent limiter did not persist")
        check(state.autosave && state.output_class === "headphones", "autosave or output class did not persist")
    }
    function advance() {
        if (captures > 0) return
        Dsp.refresh()
        const state = JSON.parse(Dsp.snapshot || "{}")
        if (!state.available || state.loading) return
        check(!state.active && !Dsp.active && !playing, "the smoke must not play audio or report idle DSP as active")
        check(state.output === "file:capture", "the isolated output was not selected")
        check(!state.error, "DSP error: " + state.error)
        check(state.curve.length === 129 && state.curve.every(Number.isFinite), "invalid response curve")
        check(Number.isFinite(state.headroom_db), "invalid headroom")
        if (step === 0) {
            ThemeModel.mode = "dark"
            check(state.providers.length === 1 && state.providers[0].id === pluginId, "wrong packaged provider")
            const loader = find(control, item => item.objectName === "dspDialogLoader")
            check(!!loader && !loader.item, "idle dialog was retained")
            control.clicked(); check(!!loader.item && loader.item.visible, "player did not open the real DSP modal")
            loader.item.close(); check(!loader.item, "closing did not release the real modal")
            panel.open()
            if (mode === "restore") { savedId = state.selection.preset; checkSaved(state); check(!state.selection.enabled, "bypass did not survive restart"); complete(state); return }
        } else if (step === 1) {
            check(panel.width >= 1200 && panel.wide, "large workspace or sidebar is missing")
            named("dspPresetPicker").picked(pluginId + "/hd650")
        } else if (step === 2) {
            check(state.preset.id === "hd650" && state.preset.filters.length === 10, "measured correction was not selected")
            originalFilters = JSON.stringify(state.preset.filters)
            named("dspFavorite").clicked()
            panel.section = "parametric"
        } else if (step === 3) {
            capture("native-parametric", () => { panel.section = "tone"; setSlider("dspKnob_bass", 3) })
        } else if (step === 4) {
            check(state.preset.tone.bass_db === 3 && JSON.stringify(state.preset.filters) === originalFilters, "bass displaced correction filters")
            setSlider("dspKnob_treble", 2)
        } else if (step === 5) {
            check(state.preset.tone.treble_db === 2 && JSON.stringify(state.preset.filters) === originalFilters, "treble displaced correction filters")
            capture("native-tone", () => { panel.section = "graphic"; named("dspStartGraphic").clicked() })
        } else if (step === 6) {
            check(state.graphic && state.preset.filters.length === 10 && state.selection.graphic_origin, "graphic EQ did not preserve correction")
            named("dspGraphicCount").picked("32")
            named("dspGraphicMinimum").text = "20"; named("dspGraphicMinimum").editingFinished()
            named("dspGraphicMaximum").text = "20000"; named("dspGraphicMaximum").editingFinished()
            named("dspRebuildGraphic").clicked()
        } else if (step === 7) {
            check(state.graphic && state.preset.filters.length === 32 && state.preset.tone.bass_db === 3, "32-band layout lost tone")
            check(Math.abs(state.preset.filters[31].frequency - 20000) < 0.001, "graphic frequency endpoint changed")
            setSlider("dspGraphicBand0", 3)
            capture("native-graphic-32")
        } else if (step === 8) {
            check(state.preset.filters[0].gain_db === 3, "graphic band did not apply")
            named("dspRestorePrevious").clicked()
        } else if (step === 9) {
            check(JSON.stringify(state.preset.filters) === originalFilters && state.preset.tone.bass_db === 3, "graphic restore changed correction or tone")
            panel.section = "presets"; panel.group = "favorites"; panel.query = ""; panel.requestSearch(0)
        } else if (step === 10) {
            if (!browserReady(state, "", "favorites")) return
            check(panel.shown.some(entry => entry.preset.id === "hd650"), "favorite query lost the measured profile")
            panel.group = "Headphones"; named("dspSearch").text = "WH1000XM4"
        } else if (step === 11) {
            if (!browserReady(state, "WH1000XM4", "Headphones")) return
            const external = panel.shown.find(entry => entry.preset.id.startsWith("autoeq-"))
            check(!!external && state.browser.total > 1, "expanded search did not return measurement variants")
            remoteId = external.preset.id
            capture("native-device-search", () => named("dspPresetBrowser").chosen(external))
        } else if (step === 12) {
            check(state.preset.id === remoteId && state.selection.resolved, "paged profile was not retained by identity")
            check(named("dspPresetPicker").current >= 0, "selected measured variant disappeared from the quick picker")
            panel.close(); panel.open()
            panel.section = "import"
            find(panel.contentItem, item => typeof item.submitted === "function").submitted("DSP smoke import", "Preamp: -7 dB\nFilter 1: ON PK Fc 110 Hz Gain 2.5 dB Q 0.8")
        } else if (step === 13) {
            check(state.preset.filters.length === 1 && state.preset.preamp_db === -7, "text import failed")
            panel.section = "tone"; setSlider("dspKnob_bass", 3); setSlider("dspKnob_treble", 2)
        } else if (step === 14) {
            panel.section = "dynamics"
            setSlider("dspParameter_compressor_ratio", 2)
            setSlider("dspParameter_compressor_threshold_db", -24)
            setSlider("dspParameter_dynamics_attack_ms", 17)
            setSlider("dspParameter_dynamics_knee_db", 6)
            setSlider("dspParameter_dynamics_pre_gain_db", 3)
            setSlider("dspParameter_dynamics_makeup_db", 2)
            setSlider("dspParameter_dynamics_sidechain_hpf_hz", 120)
            named("dspLimiterEnabled").toggled(true)
            setSlider("dspParameter_limiter_ceiling_db", -2)
        } else if (step === 15) {
            check(state.preset.dynamics.attack_ms === 17 && state.preset.limiter.ceiling_db === -2, "dynamics controls did not apply")
            check(named("dspLimiterEnabled").checked, "limiter toggle does not show its enabled state")
            capture("native-dynamics", () => { panel.section = "manage"; named("dspSaveName").text = "DSP smoke initial"; named("dspSavePreset").clicked() })
        } else if (step === 16) {
            check(state.selection.preset.startsWith("user-") && !state.selection.edited, "save-copy failed")
            savedId = state.selection.preset
            named("dspSaveName").text = savedName; named("dspRenamePreset").clicked()
            named("dspAutoSave").toggled(true)
            panel.section = "tone"; setSlider("dspKnob_bass", 4)
        } else if (step === 17) {
            check(state.preset.name === savedName && state.selection.preset === savedId && !state.selection.edited && state.preset.tone.bass_db === 4, "autosave created another identity or lost the name")
            const exported = JSON.parse(Dsp.exportPresets("[]"))
            check(exported.version === 2 && exported.presets[0].tone.bass_db === 4, "current export lost tone")
            const first = exported.presets[0], second = JSON.parse(JSON.stringify(first))
            first.name = "DSP smoke batch A"; second.name = "DSP smoke batch B"
            panel.section = "import"
            find(panel.contentItem, item => typeof item.libraryRequested === "function").libraryRequested("Batch", JSON.stringify([first, second]))
        } else if (step === 18) {
            check(state.presets.filter(entry => entry.preset.id.startsWith("user-")).length === 3, "batch import was incomplete")
            panel.section = "presets"; panel.group = "personal"; panel.query = ""; panel.requestSearch(0)
        } else if (step === 19) {
            if (!browserReady(state, "", "personal")) return
            const extra = panel.shown.filter(entry => entry.preset.id !== savedId)
            check(extra.length === 2, "personal query did not show imported batch")
            for (const entry of extra) named("dspPresetBrowser").checked(entry.preset.id, true)
        } else if (step === 20) {
            check(state.browser.selected_ids.length === 2, "batch selection was lost")
            check(JSON.parse(Dsp.exportPresets(JSON.stringify(state.browser.selected_ids))).presets.length === 2, "batch export omitted a selected preset")
            named("dspDeleteSelected").clicked()
        } else if (step === 21) {
            const dialog = named("dspPresetBrowser").deletionDialog
            check(dialog.visible && dialog.targetValid, "batch deletion did not request a valid confirmation")
            const accept = find(dialog.contentItem, item => item.objectName === "confirmationAccept")
            check(!!accept && accept.enabled, "batch confirmation is unavailable")
            accept.clicked()
        } else if (step === 22) {
            if (state.browser.loading) return
            check(state.presets.filter(entry => entry.preset.id.startsWith("user-")).length === 1, "batch deletion lost the retained preset or kept deleted entries")
            panel.section = "assignments"
            named("dspOutputClass").picked("headphones")
            named("dspAssignmentScope").picked("headphones"); named("dspAssignRule").clicked()
            named("dspAssignmentScope").picked("default"); named("dspAssignRule").clicked()
        } else if (step === 23) {
            check(state.output_class === "headphones" && state.assignments.length === 2, "class/general assignments did not persist")
            named("dspInheritAssignment").clicked()
        } else if (step === 24) {
            check(state.selection_scope === "headphones" && state.selection.preset === savedId, "device override did not fall back to its class")
            named("dspAssignDevice").clicked()
            named("dspEnabled").toggled(false)
            panel.section = "parametric"
            window.width = 960; window.height = 600
        } else if (step === 25) {
            checkSaved(state)
            check(!state.selection.enabled && state.selection_scope === "device", "bypass or device assignment was lost")
            const viewport = named("dspPageLoader"), curve = named("dspResponseCurve"), footer = named("dspDisclaimer")
            check(curve.height <= viewport.height * Metrics.dspCurveHeightShare + 1, "curve crowded the short-window editor")
            const point = footer.mapToItem(panel.contentItem, 0, 0)
            check(point.y >= 0 && point.y + footer.height <= panel.contentItem.height, "footer left the visible modal")
            capture("native-compact")
        } else if (step === 26) {
            ThemeModel.mode = "light"
            window.width = 1440; window.height = 1000
        } else if (step === 27) {
            capture("native-light-parametric")
        } else if (step === 28) {
            complete(state); return
        }
        ++step
    }
    Timer {
        interval: 300; repeat: true; running: !window.stopped
        onTriggered: { try { window.advance() } catch (error) { window.stopped = true; console.error("SMOKE FAIL DSP step=" + window.step + ": " + error.message); Qt.exit(1) } }
    }
    Timer { interval: 35000; running: !window.stopped; onTriggered: { console.error("SMOKE FAIL DSP timed out: " + Dsp.snapshot); Qt.exit(1) } }
}
