import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property var draft: ({})
    property var meters: [0, 0, 0, 0]
    readonly property var dynamics: draft.dynamics || ({enabled: true, attack_ms: 5, release_ms: 180, knee_db: 0, pre_gain_db: 0, makeup_db: 0, sidechain_hpf_hz: 0, gate_enabled: false, gate_threshold_db: -90, expander_ratio: 1})
    readonly property var limiter: draft.limiter || ({enabled: false, ceiling_db: -1, release_ms: 50})
    signal edited(var value)
    signal previewed(var value)
    signal resetMeters()
    spacing: Metrics.gap24
    function change(group, key, value, preview) {
        const next = JSON.parse(JSON.stringify(draft))
        if (group) {
            next[group] = Object.assign({}, group === "dynamics" ? dynamics : limiter)
            next[group][key] = value
        } else next[key] = value
        preview ? previewed(next) : edited(next)
    }
    Text {
        width: parent.width
        text: I18n.tr("Linked peak compression keeps stereo balance. The sidechain high-pass changes detection only. The separate limiter controls sample peaks, including makeup gain; it is not a true-peak limiter.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    Flow {
        width: parent.width
        spacing: Metrics.dspSectionGap
        Column {
            width: editor.width < Metrics.dspFilterMinWidth ? parent.width : (parent.width - parent.spacing) / 2 // derived: one column on small editors
            spacing: Metrics.gap20
            Row {
                spacing: Metrics.gap12
                Toggle { objectName: "dspDynamicsEnabled"; checked: editor.dynamics.enabled; onToggled: on => editor.change("dynamics", "enabled", on, false) }
                Text { text: I18n.tr("Compressor and expander", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.section }
            }
            Repeater {
                model: [
                    {group: "", key: "compressor_threshold_db", label: I18n.tr("Threshold", I18n.rev), unit: "dB", low: -90, high: 0, step: 0.5},
                    {group: "", key: "compressor_ratio", label: I18n.tr("Ratio", I18n.rev), unit: ":1", low: 1, high: 10, step: 0.1},
                    {group: "dynamics", key: "attack_ms", label: I18n.tr("Attack", I18n.rev), unit: "ms", low: 1, high: 120, step: 1},
                    {group: "dynamics", key: "release_ms", label: I18n.tr("Release", I18n.rev), unit: "ms", low: 1, high: 500, step: 1},
                    {group: "dynamics", key: "knee_db", label: I18n.tr("Knee", I18n.rev), unit: "dB", low: 0, high: 20, step: 0.5},
                    {group: "dynamics", key: "pre_gain_db", label: I18n.tr("Pre-gain", I18n.rev), unit: "dB", low: 0, high: 45, step: 0.5},
                    {group: "dynamics", key: "makeup_db", label: I18n.tr("Makeup gain", I18n.rev), unit: "dB", low: 0, high: 45, step: 0.5},
                    {group: "dynamics", key: "sidechain_hpf_hz", label: I18n.tr("Sidechain high-pass", I18n.rev), unit: "Hz", low: 0, high: 20000, step: 10},
                    {group: "dynamics", key: "gate_threshold_db", label: I18n.tr("Gate / expander threshold", I18n.rev), unit: "dB", low: -90, high: 0, step: 0.5},
                    {group: "dynamics", key: "expander_ratio", label: I18n.tr("Expander ratio", I18n.rev), unit: ":1", low: 1, high: 10, step: 0.1}
                ]
                DspParameter {
                    required property var modelData
                    width: parent.width
                    parameterKey: modelData.group ? modelData.group + "_" + modelData.key : modelData.key
                    title: modelData.label
                    unit: modelData.unit
                    minimum: modelData.low
                    maximum: modelData.high
                    step: modelData.step
                    value: Number(modelData.group ? editor.dynamics[modelData.key] : editor.draft[modelData.key] ?? 1)
                    enabled: editor.dynamics.enabled
                    onPreviewed: value => editor.change(modelData.group, modelData.key, value, true)
                    onCommitted: value => editor.change(modelData.group, modelData.key, value, false)
                }
            }
            Row {
                spacing: Metrics.gap12
                Toggle { checked: !!editor.dynamics.gate_enabled; onToggled: on => editor.change("dynamics", "gate_enabled", on, false) }
                Text { text: I18n.tr("Gate below threshold", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body }
            }
        }
        Column {
            width: editor.width < Metrics.dspFilterMinWidth ? parent.width : (parent.width - parent.spacing) / 2 // derived: one column on small editors
            spacing: Metrics.gap20
            Row {
                spacing: Metrics.gap12
                Toggle { objectName: "dspLimiterEnabled"; checked: editor.limiter.enabled; onToggled: on => editor.change("limiter", "enabled", on, false) }
                Text { text: I18n.tr("Limiter", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.section }
            }
            Repeater {
                model: [
                    {key: "ceiling_db", label: I18n.tr("Ceiling", I18n.rev), unit: "dBFS", low: -24, high: 0, step: 0.1},
                    {key: "release_ms", label: I18n.tr("Release", I18n.rev), unit: "ms", low: 1, high: 500, step: 1}
                ]
                DspParameter {
                    required property var modelData
                    width: parent.width
                    parameterKey: "limiter_" + modelData.key
                    title: modelData.label
                    unit: modelData.unit
                    minimum: modelData.low
                    maximum: modelData.high
                    step: modelData.step
                    value: Number(editor.limiter[modelData.key])
                    enabled: editor.limiter.enabled
                    onPreviewed: value => editor.change("limiter", modelData.key, value, true)
                    onCommitted: value => editor.change("limiter", modelData.key, value, false)
                }
            }
            Text { width: parent.width; text: I18n.tr("The limiter adds no delay. Automatic EQ headroom does not cancel compressor pre-gain or makeup gain.", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textTertiary; font.pointSize: Type.small }
            DspMeters { width: parent.width; values: editor.meters; onResetRequested: editor.resetMeters() }
        }
    }
}
