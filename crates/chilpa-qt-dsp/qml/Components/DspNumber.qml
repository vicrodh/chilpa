import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

TextField {
    id: field
    property real value: 0
    signal committed(real value)
    width: Metrics.dspBandValueWidth
    function showValue() { text = String(Number(value.toFixed(3))) }
    Component.onCompleted: showValue()
    onValueChanged: if (!editing) showValue()
    onEditingFinished: {
        const number = Number(text)
        if (text.trim() !== "" && Number.isFinite(number) && number !== value) committed(number)
        else showValue()
    }
}
