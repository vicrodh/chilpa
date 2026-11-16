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
    DspHeader { width: parent.width; available: true }
    DspNavigation {
        width: Metrics.dspSidebarWidth
        height: Metrics.dspSliderHeight
        sections: panel.sections
    }
    DspPresetBar { width: parent.width; presets: [{plugin: "example", preset: parent.sample}] }
    DspPresets { width: parent.width; height: Metrics.dspSliderHeight; presets: [{plugin: "example", preset: parent.sample}]; groups: [{key: "all", label: "All"}] }
    DspFooter { width: parent.width; presetName: parent.sample.name }
    DspControl {}
    DspQualityNote { width: parent.width }
    PillButton { text: "DSP editor"; onClicked: panel.open() }
    DspNumber { value: 1000 }
    DspGraphic { width: parent.width; draft: parent.sample; graphic: false }
    DspEditor { width: parent.width; draft: parent.sample; state: ({selection: {}, curve: [], clipped: 0}) }
    DspGain { width: parent.width; preamp: -6; headroom: -7.5; clipped: 0 }
    DspEqualizer { width: parent.width; draft: parent.sample; curve: [0, 0.5, 2, 0.5, 0] }
    DspStereo { width: parent.width; draft: parent.sample }
    DspTone { width: parent.width; draft: parent.sample }
    DspKnob { width: Metrics.dspSidebarWidth; title: "Bass"; value: 3; maximum: 22.5 }
    DspParameter { width: parent.width; title: "Attack"; unit: "ms"; minimum: 1; maximum: 120; value: 5 }
    DspCurve { width: parent.width; values: [0, 2, -1, 0]; filters: parent.sample.filters; interactive: true }
    DspFilterRow { width: parent.width; bandIndex: 0; filter: parent.sample.filters[0] }
    DspGraphicBand { width: Metrics.dspBandValueWidth; frequency: 1000; gain: 3 }
    DspGraphicLayout { width: parent.width }
    DspDynamics { width: parent.width; draft: parent.sample; meters: [0.5, 0.25, 4, 1] }
    DspMeters { width: parent.width; values: [0.5, 0.25, 4, 1] }
    DspAssignments { width: parent.width; state: ({output_name: "Example DAC", selection_scope: "device"}) }
    DspEditQueue {}
    DspPersonal { width: parent.width; presetName: parent.sample.name; selection: ({preset: "user-1", edited: parent.sample}) }
    DspImport { width: parent.width }
}
