import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The floating player (design/02 7): transport, the "LCD" with what plays,
// and the actions. Only controls that work are shown; cast, lyrics and the
// queue join when they exist.
Item {
    id: bar

    implicitHeight: Metrics.playerHeight

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
        radius: Metrics.radiusPlayer
        color: Theme.color.surfacePlayer
        border.color: Theme.color.strokeHairline
    }

    Row {
        id: transport
        anchors.left: parent.left
        anchors.leftMargin: Metrics.playerPadLeft
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap10

        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Shuffle", I18n.rev)
            glyph: Icons.arrowsShuffle
            tone: Player.shuffle ? "accent" : "secondary"
            onClicked: Player.toggleShuffle()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Previous", I18n.rev)
            glyph: Icons.playerSkipBackFilled
            filled: true
            iconSize: Metrics.iconSkip
            tone: "primary"
            onClicked: Player.previous()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: Player.playing && !Player.paused ? I18n.tr("Pause", I18n.rev) : I18n.tr("Play", I18n.rev)
            glyph: Player.playing && !Player.paused ? Icons.playerPauseFilled : Icons.playerPlayFilled
            filled: true
            size: Metrics.button40
            iconSize: Metrics.iconPlay
            tone: "primary"
            onClicked: Player.toggle()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Next", I18n.rev)
            glyph: Icons.playerSkipForwardFilled
            filled: true
            iconSize: Metrics.iconSkip
            tone: "primary"
            onClicked: Player.next()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Repeat", I18n.rev)
            glyph: Player.repeat === 2 ? Icons.repeatOnce : Icons.repeat
            tone: Player.repeat > 0 ? "accent" : "secondary"
            onClicked: Player.cycleRepeat()
        }
    }

    PlayerLcd {
        anchors.left: transport.right
        anchors.leftMargin: Metrics.gap16
        anchors.right: parent.right
        anchors.rightMargin: Metrics.playerPadRight
        anchors.verticalCenter: parent.verticalCenter
        showTimes: bar.width >= Metrics.playerNoTimes
    }
}
