import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One compact speaker in the player bar opens the complete volume control
// upward. A direct route with no knob to move stays locked and explains why.
Item {
    id: control

    implicitWidth: Metrics.button28
    implicitHeight: Metrics.button28

    IconButton {
        id: opener
        anchors.fill: parent
        opacity: Player.volumeLocked ? Metrics.disabledOpacity : 1
        glyph: Player.muted || Player.volume === 0 ? Icons.volume3 : Player.volume < 50 ? Icons.volume2 : Icons.volume
        iconSize: Metrics.iconNav
        tip: Player.volumeLocked ? Player.volumeNote
            : I18n.trArgs("Volume {}%", [String(Player.muted ? 0 : Player.volume)], I18n.rev)
        onClicked: {
            if (!Player.volumeLocked)
                volume.opened ? volume.close() : volume.open()
        }
    }

    Popup {
        id: volume

        x: (control.width - width) / 2 // derived: centered over its button
        y: -height - Metrics.gap8
        width: Metrics.volumePopupWidth
        padding: Metrics.gap8
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent

        background: Item {
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
            spacing: Metrics.gap6

            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: String(Player.muted ? 0 : Player.volume) + "%"
                font.pointSize: Type.micro
                font.features: { "tnum": 1 }
                color: Theme.color.textSecondary
            }
            LevelSlider {
                anchors.horizontalCenter: parent.horizontalCenter
                vertical: true
                enabled: !Player.volumeLocked
                value: Player.muted ? 0 : Player.volume
                tip: I18n.trArgs("Volume {}%", [String(Player.volume)], I18n.rev)
                onMoved: (level) => {
                    // Moving it is wanting sound: a muted volume comes back.
                    if (Player.muted)
                        Player.chooseMuted(false)
                    Player.chooseVolume(level)
                }
            }
            IconButton {
                anchors.horizontalCenter: parent.horizontalCenter
                size: Metrics.button24
                glyph: Player.muted ? Icons.volume3 : Icons.volume
                tip: Player.muted ? I18n.tr("Unmute", I18n.rev) : I18n.tr("Mute", I18n.rev)
                checked: Player.muted
                onClicked: Player.chooseMuted(!Player.muted)
            }
        }
    }
}
