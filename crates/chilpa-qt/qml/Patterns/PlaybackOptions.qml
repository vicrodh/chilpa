import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Parts

// How the queue plays, from the player bar: gapless, one loudness for every
// track, and the mix by tempo -- the switches Settings has.
Popup {
    id: options

    // A click outside closes it and is nothing else: it is not also a click
    // on what is there.
    modal: true
    dim: false

    padding: Metrics.gap12
    width: Metrics.playbackOptionsWidth
    margins: Metrics.gap10
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Item {
        Hold {}
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised

            Edge {}
        }
    }

    contentItem: Column {
        spacing: Metrics.gap4

        OptionRow {
            width: parent.width
            title: I18n.tr("Gapless", I18n.rev)
            detail: I18n.tr("One track straight into the next.", I18n.rev)
            available: !Cast.active
            checked: Player.gapless
            onToggled: (on) => Player.chooseGapless(on)
        }
        OptionRow {
            width: parent.width
            title: I18n.tr("Normalization", I18n.rev)
            detail: I18n.tr("Every track at one loudness, by its ReplayGain. Changed tracks are reported as processed.", I18n.rev)
            checked: Player.normalize
            available: !Cast.active && Player.normalizeApplies
            onToggled: (on) => Player.chooseNormalize(on)
        }
        // Offered only when the library has tracks whose tempo was read.
        OptionRow {
            visible: Player.automixAvailable
            width: parent.width
            title: I18n.tr("AutoMix", I18n.rev)
            detail: Player.automixNote !== "" ? Player.automixNote
                : I18n.tr("The queue in the order its tempo flows best. Off, back to its own.", I18n.rev)
            checked: Player.automix
            onToggled: (on) => Player.chooseAutomix(on)
        }
        Parts.LoopRow {
            visible: !Cast.active && Player.duration > 0
            width: parent.width
        }
    }
}
