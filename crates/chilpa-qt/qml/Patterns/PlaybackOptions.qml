import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// How the queue plays, from the player bar.
Popup {
    id: options
    PopupInput { surface: options }

    // A click outside closes it and is nothing else: it is not also a click
    // on what is there.
    modal: true
    dim: false

    padding: Metrics.gap12
    width: Metrics.playbackOptionsWidth
    margins: Metrics.gap10
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

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
            title: I18n.tr("Volume normalization", I18n.rev)
            detail: I18n.tr("Level loudness across tracks. This breaks strict bit perfect.", I18n.rev)
            checked: Player.normalize
            available: !Cast.active && Player.normalizeApplies
            onToggled: (on) => Player.chooseNormalize(on)
        }
    }
}
