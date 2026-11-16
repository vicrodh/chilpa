import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: parameter
    property bool initialized: false
    Component.onCompleted: initialized = true
    property string title: ""
    property string parameterKey: title
    property string unit: ""
    property real value: 0
    property real minimum: 0
    property real maximum: 1
    property real step: 0.1
    signal previewed(real value)
    signal committed(real value)
    spacing: Metrics.gap6
    Text { width: parent.width; text: parameter.title + (parameter.unit ? " · " + parameter.unit : ""); font.pointSize: Type.body; color: Theme.color.textPrimary }
    Row {
        width: parent.width
        spacing: Metrics.gap16
        Controls.Slider {
            id: slider
            objectName: "dspParameter_" + parameter.parameterKey
            width: Math.max(0, parent.width - number.width - parent.spacing)
            from: parameter.minimum
            to: parameter.maximum
            stepSize: parameter.step
            property bool gestureChanged: false
            property real gestureValue: 0
            Binding { restoreMode: Binding.RestoreNone; target: slider; property: "value"; value: parameter.value; when: parameter.initialized && !slider.pressed }
            wheelEnabled: activeFocus
            Accessible.name: parameter.title
            onMoved: { if (pressed) { gestureChanged = true; gestureValue = value; parameter.previewed(value) } else parameter.committed(value) }
            onPressedChanged: if (!pressed && gestureChanged) { gestureChanged = false; parameter.committed(gestureValue) }
            background: Rectangle {
                x: slider.leftPadding
                y: (slider.height - height) / 2
                width: slider.availableWidth
                height: Metrics.gap4
                radius: height / 2 // derived: rounded track
                color: Theme.color.stateSelected
                Rectangle { width: slider.position * parent.width; height: parent.height; radius: parent.radius; color: Theme.color.accent }
            }
            handle: Rectangle {
                x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
                y: (slider.height - height) / 2
                width: Metrics.dspSliderHandle
                height: width
                radius: width / 2 // derived: circular handle
                color: slider.pressed ? Theme.color.accent : Theme.color.textPrimary
                border.width: slider.activeFocus ? Metrics.focusRing : 0
                border.color: Theme.color.accent
            }
        }
        DspNumber {
            id: number
            value: parameter.value
            minimum: parameter.minimum
            maximum: parameter.maximum
            label: parameter.title + " " + parameter.unit
            onCommitted: value => parameter.committed(value)
        }
    }
}
