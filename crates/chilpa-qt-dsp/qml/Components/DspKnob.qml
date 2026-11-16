import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: knob
    property bool initialized: false
    Component.onCompleted: initialized = true
    property string title: ""
    property string controlKey: title
    property real value: 0
    property real minimum: -24
    property real maximum: 15
    signal previewed(real value)
    signal committed(real value)
    spacing: Metrics.gap12
    Text { width: parent.width; text: knob.title; horizontalAlignment: Text.AlignHCenter; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary } // derived: rounded indicator
    Controls.Dial {
        id: dial
        objectName: "dspKnob_" + knob.controlKey
        anchors.horizontalCenter: parent.horizontalCenter
        width: Metrics.dspKnobSize
        height: width
        from: knob.minimum
        to: knob.maximum
        stepSize: 0.5
        property bool gestureChanged: false
        property real gestureValue: 0
        Binding { restoreMode: Binding.RestoreNone; target: dial; property: "value"; value: knob.value; when: knob.initialized && !dial.pressed }
        wrap: false
        wheelEnabled: activeFocus
        Accessible.name: knob.title
        onMoved: { if (pressed) { gestureChanged = true; gestureValue = value; knob.previewed(value) } else knob.committed(value) }
        onPressedChanged: if (!pressed && gestureChanged) { gestureChanged = false; knob.committed(gestureValue) }
        background: Canvas {
            anchors.fill: parent
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            Connections { target: dial; function onAngleChanged() { arc.requestPaint() } }
            Connections { target: Theme; function onRevChanged() { arc.requestPaint() } }
            id: arc
            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                const radius = width / 2 - Metrics.gap8
                ctx.lineWidth = Metrics.gap4
                ctx.lineCap = "round"
                ctx.strokeStyle = Theme.color.strokeHairline
                ctx.beginPath(); ctx.arc(width / 2, height / 2, radius, -230 * Math.PI / 180, 50 * Math.PI / 180); ctx.stroke()
                ctx.strokeStyle = Theme.color.accent
                ctx.beginPath(); ctx.arc(width / 2, height / 2, radius, -230 * Math.PI / 180, (dial.angle - 90) * Math.PI / 180); ctx.stroke()
            }
        }
        handle: Item {
            x: dial.width / 2 - width / 2
            y: dial.height / 2 - height / 2
            width: dial.width - Metrics.gap24
            height: width
            rotation: dial.angle
            Rectangle { anchors.horizontalCenter: parent.horizontalCenter; y: Metrics.gap8; width: Metrics.gap4; height: Metrics.dspKnobIndicator; radius: width / 2; color: Theme.color.textPrimary } // derived: rounded indicator
        }
        FocusRing {}
    }
    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Metrics.gap8
        DspNumber { value: knob.value; minimum: knob.minimum; maximum: knob.maximum; label: knob.title + " dB"; onCommitted: value => knob.committed(value) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: "dB"; color: Theme.color.textSecondary; font.pointSize: Type.body }
    }
    PillButton { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.tr("Reset", I18n.rev); onClicked: knob.committed(0) }
}
