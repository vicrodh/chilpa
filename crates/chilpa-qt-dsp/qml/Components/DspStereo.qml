import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property var draft: ({})
    signal edited(var value)
    spacing: Metrics.gap6
    function patch(key, value) {
        const next = JSON.parse(JSON.stringify(draft))
        next[key] = value
        edited(next)
    }
    Text {
        width: parent.width
        text: I18n.tr("Stereo controls apply to two-channel PCM. EQ and linked compression apply to all channels.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textTertiary
        font.pointSize: Type.small
    }
    Repeater {
        model: [
            { key: "crossfeed", label: I18n.tr("Crossfeed · 0–50%", I18n.rev), factor: 100 },
            { key: "width", label: I18n.tr("Stereo width · 0–100%", I18n.rev), factor: 100 },
            { key: "balance", label: I18n.tr("Balance · −100 left / +100 right", I18n.rev), factor: 100 },
            { key: "delay_left_ms", label: I18n.tr("Left delay · 0–30 ms", I18n.rev), factor: 1 },
            { key: "delay_right_ms", label: I18n.tr("Right delay · 0–30 ms", I18n.rev), factor: 1 },
            { key: "compressor_threshold_db", label: I18n.tr("Compressor threshold · −60–0 dB", I18n.rev), factor: 1 },
            { key: "compressor_ratio", label: I18n.tr("Compressor ratio · 1–6", I18n.rev), factor: 1 }
        ]
        Row {
            id: option
            required property var modelData
            width: editor.width
            spacing: Metrics.gap8
            Text { width: parent.width - Metrics.dspBandValueWidth - parent.spacing; anchors.verticalCenter: parent.verticalCenter; text: option.modelData.label; color: Theme.color.textPrimary; font.pointSize: Type.body }
            DspNumber { value: (editor.draft[option.modelData.key] || 0) * option.modelData.factor; onCommitted: value => editor.patch(option.modelData.key, value / option.modelData.factor) }
        }
    }
    Repeater {
        model: [{ key: "invert_left", label: I18n.tr("Invert left polarity", I18n.rev) }, { key: "invert_right", label: I18n.tr("Invert right polarity", I18n.rev) }]
        Row {
            id: polarity
            required property var modelData
            width: editor.width
            spacing: Metrics.gap8
            Text { width: parent.width - polaritySwitch.width - parent.spacing; text: polarity.modelData.label; color: Theme.color.textPrimary; font.pointSize: Type.body }
            Toggle { id: polaritySwitch; checked: !!editor.draft[polarity.modelData.key]; onToggled: on => editor.patch(polarity.modelData.key, on) }
        }
    }
    Text {
        width: parent.width
        text: I18n.tr("Automatic headroom only reduces gain. Compression has no makeup gain; ratio 1 disables it.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }
}
