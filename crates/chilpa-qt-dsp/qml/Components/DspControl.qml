import QtQuick
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

IconButton {
    objectName: "playerDsp"
    visible: Dsp.available
    tip: Dsp.active ? I18n.tr("DSP active", I18n.rev) + ": " + Dsp.presetName : I18n.tr("DSP · Equalizer and presets", I18n.rev)
    glyph: Lucide.slidersVertical
    iconFamily: Lucide.font
    activeFocusOnTab: visible && enabled
    Accessible.name: tip
    Accessible.role: Accessible.Button
    Keys.onReturnPressed: clicked()
    Keys.onSpacePressed: clicked()
    checked: Dsp.active || !!dialog.item?.opened
    onClicked: {
        if (dialog.item) dialog.item.opened ? dialog.item.close() : dialog.item.open()
        else dialog.active = true
    }
    Loader {
        id: dialog
        objectName: "dspDialogLoader"
        active: false
        source: Qt.resolvedUrl("DspPanel.qml")
        onLoaded: {
            item.closed.connect(() => dialog.active = false)
            item.open()
        }
    }
    Timer { interval: Motion.refresh; repeat: true; running: true; triggeredOnStart: true; onTriggered: Dsp.refresh() }
}
