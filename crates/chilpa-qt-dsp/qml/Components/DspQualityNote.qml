import QtQuick
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Text {
    objectName: "qualityDspNote"
    property bool casting: false
    visible: Dsp.active && !casting
    text: I18n.tr("DSP active", I18n.rev) + ": " + Dsp.presetName + "\n" + I18n.tr("DSP changes the signal and breaks strict bit-perfect playback.", I18n.rev)
    wrapMode: Text.WordWrap
    font.pointSize: Type.small
    color: Theme.color.textSecondary
}
