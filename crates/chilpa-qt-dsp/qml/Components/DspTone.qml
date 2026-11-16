import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    objectName: "dspTone"
    property var draft: ({})
    readonly property var tone: draft.tone || ({ enabled: true, bass_db: 0, bass_hz: 90, bass_q: 0.8, treble_db: 0, treble_hz: 10000, treble_q: 0.6 })
    signal edited(var value)
    signal previewed(var value)
    spacing: Metrics.gap24
    function change(key, value, preview) {
        const next = JSON.parse(JSON.stringify(draft))
        next.tone = Object.assign({}, tone)
        next.tone[key] = value
        preview ? previewed(next) : edited(next)
    }
    Row {
        width: parent.width
        spacing: Metrics.gap12
        Toggle { objectName: "dspToneEnabled"; checked: editor.tone.enabled !== false; onToggled: on => editor.change("enabled", on, false) }
        Text { text: I18n.tr("Tone enabled", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body }
    }
    Text {
        width: parent.width
        text: I18n.tr("Bass and treble work alongside your headphone correction. Zero dB is neutral; EQ filters stay intact.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    Row {
        width: parent.width
        spacing: Metrics.dspSectionGap
        Repeater {
            model: [
                {key: "bass", title: I18n.tr("Bass", I18n.rev), maximum: 22.5, low: 20, high: 250},
                {key: "treble", title: I18n.tr("Treble", I18n.rev), maximum: 15, low: 5000, high: 15000}
            ]
            Column {
                id: stage
                required property var modelData
                width: (parent.width - parent.spacing) / 2 // derived: two independent tone controls
                spacing: Metrics.gap24
                DspKnob {
                    width: parent.width
                    controlKey: stage.modelData.key
                    title: stage.modelData.title
                    value: Number(editor.tone[stage.modelData.key + "_db"] || 0)
                    maximum: stage.modelData.maximum
                    enabled: editor.tone.enabled !== false
                    onPreviewed: value => editor.change(stage.modelData.key + "_db", value, true)
                    onCommitted: value => editor.change(stage.modelData.key + "_db", value, false)
                }
                DspParameter {
                    width: parent.width
                    title: I18n.tr("Frequency", I18n.rev)
                    unit: "Hz"
                    minimum: stage.modelData.low
                    maximum: stage.modelData.high
                    step: 1
                    value: Number(editor.tone[stage.modelData.key + "_hz"])
                    onPreviewed: value => editor.change(stage.modelData.key + "_hz", value, true)
                    onCommitted: value => editor.change(stage.modelData.key + "_hz", value, false)
                }
                DspParameter {
                    width: parent.width
                    title: I18n.tr("Q", I18n.rev)
                    minimum: 0.1
                    maximum: 2
                    step: 0.01
                    value: Number(editor.tone[stage.modelData.key + "_q"])
                    onPreviewed: value => editor.change(stage.modelData.key + "_q", value, true)
                    onCommitted: value => editor.change(stage.modelData.key + "_q", value, false)
                }
            }
        }
    }
}
