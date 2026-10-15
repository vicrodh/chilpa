import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/dsp/qml/Components" as Parts

Window {
    id: window
    visible: true
    width: 1100
    height: 1000
    property string pluginId: "PLUGIN_ID"
    property string mode: "SMOKE_MODE"
    property int step: 0
    property var choices: []
    property string favoriteKey: ""
    property string savedId: ""
    property string savedName: "DSP smoke personal preset"
    property bool stopped: false
    // I mount the native player poller before waiting for asynchronous plugin startup.
    readonly property bool playing: Player.playing

    Parts.DspControl { id: control }
    Parts.DspPanel { id: panel }

    function check(condition, message) {
        if (!condition)
            throw new Error(message)
    }
    function find(item, predicate) {
        if (predicate(item))
            return item
        for (const child of item.children || []) {
            const match = find(child, predicate)
            if (match)
                return match
        }
        return null
    }
    function named(name) {
        const item = find(panel.contentItem, item => item.objectName === name)
        check(!!item, "missing real editor control: " + name)
        return item
    }
    function inactive(state) {
        check(state.active === false && Dsp.active === false, "idle DSP reported active")
        check(!playing, "the smoke must not start playback")
        check(state.output === "file:capture", "the isolated file output was not selected")
        check((state.curve || []).length === 129 && state.curve.every(Number.isFinite), "invalid response curve")
        check(Number.isFinite(state.headroom_db) && state.headroom_db <= 0, "invalid automatic headroom")
        check(!state.error, "DSP error: " + state.error)
    }
    function complete(state) {
        stopped = true
        panel.close()
        console.log("SMOKE DSP snapshot " + JSON.stringify(state))
        console.log("SMOKE DSP " + mode + " completed: presets=" + state.presets.length + ", active=false")
        Qt.quit()
    }
    function advance() {
        Dsp.refresh()
        const state = JSON.parse(Dsp.snapshot || "{}")
        if (!state.available || state.loading)
            return
        inactive(state)
        if (step === 0) {
            check(Dsp.available && control.visible, "the real DSP control is hidden")
            check(state.providers.length === 1 && state.providers[0].id === pluginId,
                  "the host did not load the exact installed DSP provider")
            const catalog = state.presets.filter(entry => entry.plugin === pluginId && !entry.preset.id.startsWith("user-"))
            check(catalog.length >= 50, "the packaged DSP catalog has fewer than 50 presets")
            choices = catalog.filter(entry => entry.preset.filters.length > 0).slice(0, 2)
            check(choices.length === 2, "the catalog needs two EQ presets")
            favoriteKey = pluginId + "/" + choices[0].preset.id
            panel.open()
            step = 1
            return
        }
        check(panel.opened, "the real DSP panel did not open")
        if (mode === "restore") {
            check(state.favorites.includes(favoriteKey), "the favorite did not survive restart")
            check(state.preset.name === savedName && state.preset.id.startsWith("user-"),
                  "the saved selection did not survive restart")
            check(state.selection.enabled === false && !state.selection.edited, "bypass did not survive restart")
            check(state.preset.preamp_db === -7 && state.preset.filters.length === 1,
                  "the imported EQ did not survive restart")
            check(state.preset.filters[0].frequency === 110 && state.preset.filters[0].gain_db === 2.5,
                  "the imported filter changed after restart")
            complete(state)
            return
        }
        if (step === 1) {
            check(named("dspDisclaimer").text.includes("bit-perfect"), "missing DSP quality disclaimer")
            check(panel.allPresets.length === state.presets.length, "the panel omitted catalog entries")
            named("dspPresetPicker").picked(pluginId + "/" + choices[0].preset.id)
        } else if (step === 2) {
            check(state.selection.preset === choices[0].preset.id && state.selection.enabled,
                  "the real picker did not choose the first preset")
            named("dspFavorite").clicked()
        } else if (step === 3) {
            check(state.favorites.includes(favoriteKey) && named("dspFavorite").checked,
                  "the real favorite control did not persist its choice")
            panel.group = "favorites"
        } else if (step === 4) {
            check(panel.shown.length === 1 && panel.shown[0].preset.id === choices[0].preset.id,
                  "the favorite group did not filter the catalog")
            panel.group = "all"
            named("dspSearch").text = "no-matching-dsp-smoke-preset"
        } else if (step === 5) {
            check(panel.shown.length === 0, "the real search did not filter the catalog")
            named("dspSearch").text = ""
        } else if (step === 6) {
            named("dspPresetPicker").picked(pluginId + "/" + choices[1].preset.id)
        } else if (step === 7) {
            check(state.selection.preset === choices[1].preset.id, "the real picker did not change presets")
            const edited = JSON.parse(JSON.stringify(state.preset))
            edited.preamp_db = -9
            edited.balance = 0.15
            panel.edit(edited)
        } else if (step === 8) {
            check(!!state.selection.edited && state.preset.preamp_db === -9 && state.preset.balance === 0.15,
                  "the editor did not apply its draft")
            panel.tab = 2
        } else if (step === 9) {
            const importer = find(panel.contentItem, item => typeof item.submitted === "function")
            check(!!importer, "the real import editor did not mount")
            importer.submitted("DSP smoke import", "Preamp: -7 dB\nFilter 1: ON PK Fc 110 Hz Gain 2.5 dB Q 0.8")
        } else if (step === 10) {
            check(state.preset.name === "DSP smoke import" && state.preset.preamp_db === -7,
                  "the real import editor did not apply the profile")
            check(state.preset.filters.length === 1 && state.preset.filters[0].frequency === 110,
                  "the imported filter is missing")
            const nameField = find(panel.contentItem,
                                   item => item.placeholder === I18n.tr("Name a personal preset", I18n.rev))
            check(!!nameField, "the real personal preset name field did not mount")
            nameField.text = savedName
        } else if (step === 11) {
            const saveButton = find(panel.contentItem,
                                    item => item.text === I18n.tr("Save preset", I18n.rev) && typeof item.clicked === "function")
            check(!!saveButton && saveButton.enabled, "the real save preset button is unavailable")
            saveButton.clicked()
        } else if (step === 12) {
            check(state.preset.name === savedName && state.selection.preset.startsWith("user-") && !state.selection.edited,
                  "the editor did not save a personal preset")
            savedId = state.selection.preset
            named("dspEnabled").toggled(false)
        } else if (step === 13) {
            check(state.selection.enabled === false && state.selection.preset === savedId,
                  "the real bypass control did not retain the saved preset")
            check(state.favorites.includes(favoriteKey), "editing lost the favorite")
            complete(state)
        }
        ++step
    }
    Timer {
        interval: 120
        repeat: true
        running: !window.stopped
        onTriggered: {
            try {
                window.advance()
            } catch (error) {
                window.stopped = true
                console.error("SMOKE FAIL DSP step=" + window.step + ": " + error.message)
                Qt.exit(1)
            }
        }
    }
    Timer {
        interval: 35000
        running: !window.stopped
        onTriggered: {
            console.error("SMOKE FAIL DSP timed out: " + Dsp.snapshot)
            Qt.exit(1)
        }
    }
}
