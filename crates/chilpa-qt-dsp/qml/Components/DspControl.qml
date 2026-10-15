import QtQuick
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

IconButton {
    objectName: "playerDsp"
    visible: Dsp.available
    tip: Dsp.active ? I18n.tr("DSP active", I18n.rev) + ": " + Dsp.presetName : I18n.tr("DSP · Equalizer and presets", I18n.rev)
    glyph: Icons.adjustmentsHorizontal
    checked: Dsp.active || panel.opened
    onClicked: panel.opened ? panel.close() : panel.open()
    DspPanel { id: panel }
    Timer { interval: Motion.refresh; repeat: true; running: true; triggeredOnStart: true; onTriggered: Dsp.refresh() }
}
