import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: meters
    property var values: [0, 0, 0, 0]
    signal resetRequested()
    spacing: Metrics.gap12
    Text { width: parent.width; text: I18n.tr("Peak hold · reset to measure a new passage", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.small }
    Repeater {
        model: [I18n.tr("Input", I18n.rev), I18n.tr("Output", I18n.rev), I18n.tr("Compression reduction", I18n.rev), I18n.tr("Limiter reduction", I18n.rev)]
        Column {
            id: meter
            required property int index
            required property string modelData
            readonly property real measured: Number(meters.values[index] || 0)
            readonly property real decibels: index < 2 ? 20 * Math.log10(Math.max(1e-6, measured)) : measured
            width: meters.width
            spacing: Metrics.gap6
            Row {
                width: parent.width
                Text { width: parent.width - valueLabel.width; text: meter.modelData; color: Theme.color.textSecondary; font.pointSize: Type.small }
                Text { id: valueLabel; text: (meter.index < 2 && meter.measured === 0 ? "−∞" : meter.decibels.toFixed(1)) + (meter.index < 2 ? " dBFS" : " dB"); color: Theme.color.textPrimary; font.pointSize: Type.small }
            }
            Rectangle {
                width: parent.width
                height: Metrics.gap6
                radius: height / 2 // derived: rounded meter track
                color: Theme.color.stateSelected
                Rectangle { width: parent.width * Math.max(0, Math.min(1, meter.index < 2 ? (meter.decibels + 60) / 60 : meter.decibels / 60)); height: parent.height; radius: parent.radius; color: Theme.color.accent } // derived: display decibels over a sixty-decibel meter range
            }
        }
    }
    PillButton { text: I18n.tr("Reset meters", I18n.rev); onClicked: meters.resetRequested() }
}
