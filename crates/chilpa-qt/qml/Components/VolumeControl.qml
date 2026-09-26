import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The player bar's volume (design/02 7): the speaker, which mutes, and the
// slider. It moves the device's own knob or scales the samples, as the
// player decides; a direct route with no knob to move is locked, dimmed,
// and the speaker's tooltip says why.
Row {
    id: control

    spacing: Metrics.gap4
    opacity: Player.volumeLocked ? Metrics.disabledOpacity : 1

    IconButton {
        anchors.verticalCenter: parent.verticalCenter
        glyph: Player.muted || Player.volume === 0 ? Icons.volume3 : Player.volume < 50 ? Icons.volume2 : Icons.volume
        iconSize: Metrics.iconNav
        tip: Player.volumeLocked ? Player.volumeNote
            : Player.muted ? I18n.tr("Unmute", I18n.rev) : I18n.tr("Mute", I18n.rev)
        onClicked: {
            if (!Player.volumeLocked)
                Player.chooseMuted(!Player.muted)
        }
    }

    LevelSlider {
        anchors.verticalCenter: parent.verticalCenter
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
}
