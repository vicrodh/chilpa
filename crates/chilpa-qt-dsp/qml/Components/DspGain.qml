import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property bool initialized: false
    Component.onCompleted: initialized = true
    property real preamp: 0
    property real headroom: 0
    property real clipped: 0
    property bool automatic: true
    signal automaticChangedByUser(bool enabled)
    signal previewed(real gain)
    signal committed(real gain)
    signal resetMeter()
    spacing: Metrics.gap6
    Row {
        width: parent.width
        Text {
            width: parent.width - amount.implicitWidth
            text: I18n.tr("Preamp", I18n.rev)
            color: Theme.color.textPrimary
            font.pointSize: Type.body
        }
        Text {
            id: amount
            text: preampSlider.value.toFixed(1) + " dB"
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
    }
    Controls.Slider {
        id: preampSlider
        objectName: "dspPreamp"
        width: parent.width
        from: -60
        to: 24
        stepSize: 0.5
        Binding { restoreMode: Binding.RestoreNone; target: preampSlider; property: "value"; value: editor.preamp; when: editor.initialized && !preampSlider.pressed }
        wheelEnabled: activeFocus
        property bool changedWhilePressed: false
        property real gestureValue: 0
        Accessible.name: I18n.tr("Preamp", I18n.rev)
        Accessible.description: I18n.tr("Adjust overall gain. Automatic headroom can reduce the requested gain to preserve EQ headroom.", I18n.rev)
        onMoved: {
            if (pressed) { changedWhilePressed = true; gestureValue = value; editor.previewed(value) }
            else editor.committed(value)
        }
        onPressedChanged: {
            if (!pressed && changedWhilePressed) {
                changedWhilePressed = false
                editor.committed(gestureValue)
            }
        }
        background: Rectangle {
            x: preampSlider.leftPadding
            y: (preampSlider.height - height) / 2
            width: preampSlider.availableWidth
            height: Metrics.gap4
            radius: height / 2 // derived: a rounded track
            color: Theme.color.stateSelected
            Rectangle { width: preampSlider.position * parent.width; height: parent.height; radius: parent.radius; color: Theme.color.accent }
        }
        handle: Rectangle {
            x: preampSlider.leftPadding + preampSlider.visualPosition * (preampSlider.availableWidth - width)
            y: (preampSlider.height - height) / 2
            width: Metrics.dspSliderHandle
            height: width
            radius: width / 2 // derived: a circular handle or rounded track
            color: preampSlider.pressed ? Theme.color.accent : Theme.color.textPrimary
            border.width: preampSlider.activeFocus ? Metrics.focusRing : 0
            border.color: Theme.color.accent
        }
    }
    Row {
        width: parent.width
        spacing: Metrics.gap12
        Toggle { id: automaticToggle; checked: editor.automatic; onToggled: on => editor.automaticChangedByUser(on) }
        Text { width: parent.width - automaticToggle.width - parent.spacing; text: I18n.tr("Automatic headroom", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textPrimary; font.pointSize: Type.body }
    }
    Text {
        width: parent.width
        text: I18n.tr("Calculated gain", I18n.rev) + ": " + editor.headroom.toFixed(1) + " dB · "
              + I18n.tr("Automatic headroom lowers gain to leave room for EQ boosts.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textTertiary
        font.pointSize: Type.small
    }
    Row {
        width: parent.width
        spacing: Metrics.gap8
        Text {
            width: parent.width - resetMeterButton.width - parent.spacing
            text: I18n.tr("Clipped samples", I18n.rev) + ": " + editor.clipped
            anchors.verticalCenter: parent.verticalCenter
            color: editor.clipped > 0 ? Theme.color.textPrimary : Theme.color.textTertiary
            font.pointSize: Type.small
        }
        PillButton { id: resetMeterButton; text: I18n.tr("Reset counter", I18n.rev); enabled: editor.clipped > 0; onClicked: editor.resetMeter() }
    }
}
