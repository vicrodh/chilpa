import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: layout
    property var current: ({bands: 10, minimum_hz: 31.25, maximum_hz: 16000})
    property int bands: current.bands || 10
    property real minimum: current.minimum_hz || 31.25
    property real maximum: current.maximum_hz || 16000
    signal applied(var layout)
    spacing: Metrics.gap12
    Text { width: parent.width; text: I18n.tr("Rebuilding the graphic layout starts flat and keeps the previous EQ available for restoration. Tone and stereo settings stay in place.", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.small }
    Flow {
        width: parent.width
        spacing: Metrics.gap12
        SelectField { objectName: "dspGraphicCount"; width: Metrics.dspChannelWidth; options: ["5", "10", "15", "20", "31", "32"]; values: ["5", "10", "15", "20", "31", "32"]; value: String(layout.bands); onPicked: value => layout.bands = Number(value) }
        DspNumber { objectName: "dspGraphicMinimum"; value: layout.minimum; minimum: 20; maximum: 1000; label: I18n.tr("Lowest frequency", I18n.rev) + " Hz"; onCommitted: value => layout.minimum = value }
        Text { text: "Hz —"; color: Theme.color.textSecondary; font.pointSize: Type.body }
        DspNumber { objectName: "dspGraphicMaximum"; value: layout.maximum; minimum: 5000; maximum: 24000; label: I18n.tr("Highest frequency", I18n.rev) + " Hz"; onCommitted: value => layout.maximum = value }
        Text { text: "Hz"; color: Theme.color.textSecondary; font.pointSize: Type.body }
        PillButton { objectName: "dspRebuildGraphic"; text: I18n.tr("Rebuild bands", I18n.rev); onClicked: layout.applied({bands: layout.bands, minimum_hz: layout.minimum, maximum_hz: layout.maximum}) }
    }
}
