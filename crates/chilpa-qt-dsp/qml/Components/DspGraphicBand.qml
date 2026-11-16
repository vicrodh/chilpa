import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: band
    property bool initialized: false
    Component.onCompleted: initialized = true
    property int bandIndex: 0
    property real frequency: 1000
    property real gain: 0
    property bool adjustable: true
    signal previewed(real gain)
    signal committed(real gain)
    spacing: Metrics.gap6
    z: valueTip.visible ? 1 : 0
    HoverHandler { id: hover }
    function decibels(value) { const rounded = Math.round(value * 10) / 10 || 0; return (rounded > 0 ? "+" : "") + rounded.toFixed(1) }
    Text { width: parent.width; text: band.decibels(band.adjustable ? slider.value : band.gain); horizontalAlignment: Text.AlignHCenter; color: Theme.color.textPrimary; font.pointSize: Type.small }
    Controls.Slider {
        id: slider
        objectName: "dspGraphicBand" + band.bandIndex
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(Metrics.dspSliderWidth, band.width)
        height: Metrics.dspSliderHeight
        orientation: Qt.Vertical
        from: -12
        to: 12
        stepSize: 0.5
        snapMode: Controls.Slider.SnapAlways
        enabled: band.adjustable
        wheelEnabled: activeFocus
        Binding { restoreMode: Binding.RestoreNone; target: slider; property: "value"; value: band.gain; when: band.initialized && !slider.pressed }
        property bool changedWhilePressed: false
        property real gestureValue: 0
        Accessible.name: band.frequency.toFixed(0) + " Hz"
        Accessible.description: I18n.tr("Gain in decibels. Use arrow keys to adjust; Home and End reach the limits.", I18n.rev)
        onMoved: {
            if (pressed) { changedWhilePressed = true; gestureValue = value; band.previewed(value) }
            else band.committed(value)
        }
        onPressedChanged: if (!pressed && changedWhilePressed) { changedWhilePressed = false; band.committed(gestureValue) }
        background: Item {
            x: (slider.width - width) / 2
            y: slider.topPadding
            width: Metrics.gap4
            height: slider.availableHeight
            Rectangle { anchors.fill: parent; radius: width / 2; color: Theme.color.stateSelected } // derived: rounded track
            Rectangle { width: parent.width; y: Math.min(slider.visualPosition, 0.5) * parent.height; height: Math.abs(slider.visualPosition - 0.5) * parent.height; color: Theme.color.accent; radius: width / 2 } // derived: rounded track
            Repeater {
                model: 5
                Rectangle { required property int index; x: -Metrics.gap4; y: index * parent.height / 4; width: Metrics.gap12; height: Metrics.hairline; color: index === 2 ? Theme.color.textSecondary : Theme.color.strokeHairline }
            }
        }
        handle: Rectangle {
            x: (slider.width - width) / 2
            y: slider.topPadding + slider.visualPosition * (slider.availableHeight - height)
            width: Metrics.dspSliderHandle
            height: width
            radius: width / 2 // derived: circular handle
            color: !slider.enabled ? Theme.color.textSecondary : slider.pressed ? Theme.color.accent : Theme.color.textPrimary
            border.width: slider.activeFocus ? Metrics.focusRing : 0
            border.color: Theme.color.accent
        }
        Rectangle {
            id: valueTip
            objectName: "dspGraphicValue" + band.bandIndex
            visible: hover.hovered || slider.pressed || slider.visualFocus
            readonly property real leftward: slider.handle.x - width - Metrics.gap6
            x: band.x + slider.x + leftward >= 0 ? leftward : slider.handle.x + slider.handle.width + Metrics.gap6
            y: slider.handle.y + (slider.handle.height - height) / 2 // derived: centered on the handle
            width: valueText.implicitWidth + Metrics.gap16
            height: valueText.implicitHeight + Metrics.gap10
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised
            Edge {}
            Text { id: valueText; objectName: "dspGraphicValueText" + band.bandIndex; anchors.centerIn: parent; text: band.decibels(slider.value) + " dB"; font.pointSize: Type.small; color: Theme.color.textPrimary }
        }
    }
    Text { width: parent.width; text: band.frequency < 1000 ? band.frequency.toFixed(0) : (band.frequency / 1000).toFixed(1) + "k"; horizontalAlignment: Text.AlignHCenter; color: Theme.color.textSecondary; font.pointSize: Type.small }
}
