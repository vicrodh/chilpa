import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Column {
    width: Metrics.dspPanelWidth
    spacing: Metrics.gap16
    property var sample: ({ id: "example", name: "Example", category: "Example", filters: [{ shape: "peak", frequency: 1000, gain_db: 2, q: 1 }], width: 1, compressor_ratio: 1 })
    DspPanel { id: panel }
    DspControl {}
    DspQualityNote { width: parent.width }
    PillButton { text: "DSP editor"; onClicked: panel.open() }
    DspNumber { value: 1000 }
    DspEqualizer { width: parent.width; draft: parent.sample; curve: [0, 0.5, 2, 0.5, 0] }
    DspStereo { width: parent.width; draft: parent.sample }
    DspImport { width: parent.width }
}
