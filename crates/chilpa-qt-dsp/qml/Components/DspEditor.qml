import QtQuick
import app.chilpa.kit
import app.chilpa.dsp
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Column {
    id: editor
    property var state: ({})
    property var draft: ({})
    property int tab: 0
    property int advancedTab: 0
    signal command(string action, var data)
    signal edited(var value)
    signal tabRequested(int tab)
    Clipboard { id: clipboard }
    function changeGraphicBand(index, gain) {
        if (!state.graphic) command("graphic_start", {})
        const next = JSON.parse(Dsp.snapshot).preset
        next.filters[index].gain_db = gain
        edited(next)
    }
    function resetGraphic() {
        if (!state.graphic) command("graphic_start", {})
        const next = JSON.parse(Dsp.snapshot).preset
        next.filters.forEach(filter => filter.gain_db = 0)
        edited(next)
    }
    function changePreamp(gain) {
        const next = JSON.parse(JSON.stringify(draft))
        next.preamp_db = gain
        edited(next)
    }
    width: parent.width
    spacing: Metrics.gap12
    DspGraphic {
        width: parent.width
        visible: editor.tab === 0
        draft: editor.draft
        graphic: !!editor.state.graphic
        levels: editor.state.levels || []
        onBandChanged: (index, gain) => editor.changeGraphicBand(index, gain)
        onStartRequested: editor.command("graphic_start", {})
        onAdvancedRequested: { editor.advancedTab = 0; editor.tabRequested(1) }
        onResetRequested: { resetting.restoring = false; resetting.open() }
    }
    PillButton {
        objectName: "dspRestorePrevious"
        visible: !!editor.state.selection?.graphic_origin
        text: I18n.tr("Restore previous EQ", I18n.rev)
        onClicked: { resetting.restoring = true; resetting.open() }
    }
    SegmentedControl {
        visible: editor.tab === 1
        current: editor.advancedTab
        options: [{text: I18n.tr("Precise filters", I18n.rev)}, {text: I18n.tr("Stereo and dynamics", I18n.rev)}]
        onActivated: index => editor.advancedTab = index
    }
    DspEqualizer { width: parent.width; visible: editor.tab === 1 && editor.advancedTab === 0; draft: editor.draft; curve: editor.state.curve || []; onEdited: value => editor.edited(value) }
    DspStereo { width: parent.width; visible: editor.tab === 1 && editor.advancedTab === 1; draft: editor.draft; onEdited: value => editor.edited(value) }
    DspImport { width: parent.width; visible: editor.tab === 2; onSubmitted: (name, text) => editor.command("import", { name: name, text: text }); onExported: clipboard.write(JSON.stringify(editor.draft, null, 2)) }
    DspGain {
        width: parent.width
        visible: editor.tab !== 2
        preamp: Number(editor.draft.preamp_db || 0)
        headroom: Number(editor.state.headroom_db || 0)
        clipped: Number(editor.state.clipped || 0)
        onCommitted: gain => editor.changePreamp(gain)
        onResetMeter: editor.command("reset_meter", {})
    }
    ConfirmationDialog {
        id: resetting
        objectName: "dspResetConfirmation"
        property bool restoring: false
        property string original: ""
        onAboutToShow: original = JSON.stringify(editor.draft)
        title: restoring ? I18n.tr("Restore previous EQ", I18n.rev) : I18n.tr("Reset equalizer?", I18n.rev)
        message: restoring ? I18n.tr("The current equalizer changes will be replaced with the previous preset.", I18n.rev)
            : I18n.tr("The current filter gains will be reset to zero.", I18n.rev)
        confirmText: restoring ? I18n.tr("Restore previous EQ", I18n.rev) : I18n.tr("Reset", I18n.rev)
        destructive: true
        targetValid: original === JSON.stringify(editor.draft)
        onConfirmed: restoring ? editor.command("graphic_restore", {}) : editor.resetGraphic()
    }
}
