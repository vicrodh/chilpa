import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// How the queue plays, from the player bar: bit-perfect, gapless, one
// loudness for every track, and the mix by tempo -- the switches Settings has.
// Normalizing changes the samples, so with bit-perfect on it is not applied
// and says so; bit-perfect is here to turn it off without going there.
Popup {
    id: options

    padding: Metrics.gap12
    width: Metrics.playbackOptionsWidth
    margins: Metrics.gap10
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent

    background: Item {
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
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

        // First: it is what the others that change samples wait on.
        OptionRow {
            width: parent.width
            title: I18n.tr("Bit-perfect", I18n.rev)
            detail: I18n.tr("Samples reach the device untouched. Off, normalization can be used.", I18n.rev)
            checked: Player.bitPerfectMode
            onToggled: (on) => Player.chooseBitPerfect(on)
        }

        OptionRow {
            width: parent.width
            title: I18n.tr("Gapless", I18n.rev)
            detail: I18n.tr("One track straight into the next.", I18n.rev)
            checked: Player.gapless
            onToggled: (on) => Player.chooseGapless(on)
        }
        OptionRow {
            width: parent.width
            title: I18n.tr("Normalization", I18n.rev)
            detail: Player.normalizeApplies ? I18n.tr("Every track at one loudness, by its ReplayGain.", I18n.rev)
                : I18n.tr("Not with bit-perfect on: it changes the samples.", I18n.rev)
            checked: Player.normalize
            available: Player.normalizeApplies
            onToggled: (on) => Player.chooseNormalize(on)
        }
        OptionRow {
            width: parent.width
            title: I18n.tr("AutoMix", I18n.rev)
            detail: Player.automixNote !== "" ? Player.automixNote
                : I18n.tr("The queue in the order its tempo flows best. Off, back to its own.", I18n.rev)
            checked: Player.automix
            onToggled: (on) => Player.chooseAutomix(on)
        }
    }
}
