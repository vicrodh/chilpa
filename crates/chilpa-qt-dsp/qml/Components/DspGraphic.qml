import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property var draft: ({})
    property bool graphic: false
    // The preset's level at each band, from the facade: what a preset of
    // precise filters shows here, read-only.
    property var levels: []
    readonly property bool ready: graphic || (draft.filters || []).length === 0
    readonly property var frequencies: [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
    readonly property var labels: ["31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"]
    signal bandChanged(int index, real gain)
    signal startRequested()
    signal advancedRequested()
    signal resetRequested()
    spacing: Metrics.gap10

    function gain(index) { return graphic ? Number(draft.filters?.[index]?.gain_db || 0) : Number(levels[index] || 0) }
    function decibels(value) { const rounded = Math.round(value * 10) / 10 || 0; return (rounded > 0 ? "+" : "") + rounded.toFixed(1) }
    Text {
        width: parent.width
        text: editor.ready ? I18n.tr("Shape the sound: raise a band to hear more, lower it to hear less.", I18n.rev)
                           : I18n.tr("This preset uses precise filters; each band shows its level. To move the bands, create a separate graphic EQ. You can restore your previous EQ.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    Flow {
        width: parent.width
        visible: !editor.ready
        spacing: Metrics.gap8
        PillButton { objectName: "dspStartGraphic"; text: I18n.tr("Create graphic EQ", I18n.rev); kind: "primary"; onClicked: editor.startRequested() }
        PillButton { text: I18n.tr("View precise filters", I18n.rev); onClicked: editor.advancedRequested() }
    }
    Column {
        width: parent.width
        spacing: Metrics.gap8
        Row {
            width: parent.width
            Repeater {
                model: [I18n.tr("Bass", I18n.rev), I18n.tr("Mids", I18n.rev), I18n.tr("Treble", I18n.rev)]
                Text {
                    required property string modelData
                    width: parent.width / 3 // derived: three frequency ranges
                    text: modelData
                    horizontalAlignment: Text.AlignHCenter
                    color: Theme.color.textTertiary
                    font.pointSize: Type.small
                }
            }
        }
        Row {
            width: parent.width
            Repeater {
                model: editor.frequencies
                Column {
                    id: band
                    required property int index
                    required property real modelData
                    readonly property real level: editor.ready ? gainSlider.value : editor.gain(band.index)
                    width: parent.width / editor.frequencies.length
                    spacing: Metrics.gap6
                    // Over its neighbours while it shows its value beside them.
                    z: valueTip.visible ? 1 : 0
                    HoverHandler { id: hover }
                    Text {
                        width: parent.width
                        text: editor.decibels(band.level)
                        horizontalAlignment: Text.AlignHCenter
                        color: Theme.color.textPrimary
                        font.pointSize: Type.small
                    }
                    Controls.Slider {
                        id: gainSlider
                        objectName: "dspGraphicBand" + band.index
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.min(Metrics.dspSliderWidth, band.width)
                        height: Metrics.dspSliderHeight
                        orientation: Qt.Vertical
                        from: -12
                        to: 12
                        stepSize: 0.5
                        snapMode: Controls.Slider.SnapAlways
                        enabled: editor.ready
                        wheelEnabled: activeFocus
                        value: editor.gain(band.index)
                        property bool changedWhilePressed: false
                        Accessible.name: editor.labels[band.index] + " Hz"
                        Accessible.description: I18n.tr("Gain in decibels. Use arrow keys to adjust; Home and End reach the limits.", I18n.rev)
                        onMoved: {
                            if (pressed) changedWhilePressed = true
                            else editor.bandChanged(band.index, value)
                        }
                        onPressedChanged: {
                            if (!pressed && changedWhilePressed) {
                                changedWhilePressed = false
                                editor.bandChanged(band.index, value)
                            }
                        }
                        background: Item {
                            x: (gainSlider.width - width) / 2
                            y: gainSlider.topPadding
                            width: Metrics.gap4
                            height: gainSlider.availableHeight
                            Rectangle { anchors.fill: parent; radius: width / 2; color: Theme.color.stateSelected } // derived: a rounded track
                            Rectangle {
                                width: parent.width
                                y: Math.min(gainSlider.visualPosition, 0.5) * parent.height
                                height: Math.abs(gainSlider.visualPosition - 0.5) * parent.height
                                color: Theme.color.accent
                                radius: width / 2 // derived: a circular handle or rounded track
                            }
                            Repeater {
                                model: 5
                                Rectangle {
                                    required property int index
                                    x: -Metrics.gap4
                                    y: index * parent.height / 4 // derived: five marks from −12 to +12 dB
                                    width: Metrics.gap12
                                    height: 1
                                    color: index === 2 ? Theme.color.textSecondary : Theme.color.strokeHairline
                                }
                            }
                        }
                        handle: Rectangle {
                            x: (gainSlider.width - width) / 2
                            y: gainSlider.topPadding + gainSlider.visualPosition * (gainSlider.availableHeight - height)
                            width: Metrics.dspSliderHandle
                            height: Metrics.dspSliderHandle
                            radius: width / 2 // derived: a circular handle or rounded track
                            color: !gainSlider.enabled ? Theme.color.textSecondary : gainSlider.pressed ? Theme.color.accent : Theme.color.textPrimary
                            border.width: gainSlider.activeFocus ? Metrics.focusRing : 0
                            border.color: Theme.color.accent
                        }
                        // The value beside the handle, to its left where the
                        // editor has room, while it is pointed at, dragged
                        // or moved with the keys.
                        Rectangle {
                            id: valueTip
                            objectName: "dspGraphicValue" + band.index
                            readonly property real leftward: gainSlider.handle.x - width - Metrics.gap6
                            visible: hover.hovered || gainSlider.pressed || gainSlider.visualFocus
                            x: band.x + gainSlider.x + leftward >= 0 ? leftward : gainSlider.handle.x + gainSlider.handle.width + Metrics.gap6
                            y: gainSlider.handle.y + (gainSlider.handle.height - height) / 2 // derived: centered on the handle
                            width: valueText.implicitWidth + Metrics.gap16
                            height: valueText.implicitHeight + Metrics.gap10
                            radius: Metrics.radiusMenu
                            color: Theme.color.surfaceRaised
                            Edge {}
                            Text {
                                id: valueText
                                objectName: "dspGraphicValueText" + band.index
                                anchors.centerIn: parent
                                text: editor.decibels(band.level) + " dB"
                                font.pointSize: Type.small
                                color: Theme.color.textPrimary
                            }
                        }
                    }
                    Text {
                        width: parent.width
                        text: editor.labels[band.index]
                        horizontalAlignment: Text.AlignHCenter
                        color: Theme.color.textSecondary
                        font.pointSize: Type.small
                    }
                }
            }
        }
        Row {
            width: parent.width
            Text {
                width: parent.width - (resetButton.visible ? resetButton.width : 0)
                text: I18n.tr("Frequency · Hz   /   Gain · dB   /   ±12 dB", I18n.rev)
                anchors.verticalCenter: parent.verticalCenter
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            PillButton { id: resetButton; objectName: "dspResetGraphic"; visible: editor.ready; text: I18n.tr("Reset EQ", I18n.rev); onClicked: editor.resetRequested() }
        }
    }
}
