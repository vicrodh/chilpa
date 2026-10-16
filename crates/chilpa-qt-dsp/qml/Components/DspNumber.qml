import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

TextField {
    id: field
    property real value: 0
    property real minimum: -Infinity
    property real maximum: Infinity
    property string label: ""
    signal committed(real value)
    width: Metrics.dspBandValueWidth
    Accessible.name: label
    function showValue() { text = String(Number(value.toFixed(3))) }
    Component.onCompleted: showValue()
    onValueChanged: if (!editing) showValue()
    onEditingFinished: {
        const number = Number(text.replace(",", "."))
        if (text.trim() !== "" && Number.isFinite(number) && number >= minimum && number <= maximum && number !== value)
            committed(number)
        showValue()
    }
}
