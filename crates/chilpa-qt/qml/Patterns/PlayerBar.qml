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

    // The bar floats over the content: the pointer, a click in its empty
    // space and the wheel stay here instead of reaching the list beneath.
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onWheel: (wheel) => wheel.accepted = true
    }

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

        Edge {}
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
            visible: SettingsModel.extraControls
            tip: I18n.tr("Back 10 seconds", I18n.rev)
            glyph: Icons.rewindBackward10
            onClicked: Player.nudge(-10)
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
            visible: SettingsModel.extraControls
            tip: I18n.tr("Forward 10 seconds", I18n.rev)
            glyph: Icons.rewindForward10
            onClicked: Player.nudge(10)
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Repeat", I18n.rev)
            glyph: Player.repeat === 2 ? Icons.repeatOnce : Icons.repeat
            tone: Player.repeat > 0 ? "accent" : "secondary"
            onClicked: Player.cycleRepeat()
        }
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: Metrics.playerPadRight
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4

        IconButton {
            id: optionsButton
            tip: I18n.tr("Playback options", I18n.rev)
            glyph: Icons.adjustmentsHorizontal
            // Lit when something differs from how it came.
            checked: options.opened || !Player.gapless || (Player.normalize && Player.normalizeApplies) || Player.automix
            onClicked: options.opened ? options.close() : options.open()

            PlaybackOptions {
                id: options
                x: optionsButton.width - width
                y: -height - Metrics.gap10
            }
        }
        IconButton {
            tip: I18n.tr("Lyrics", I18n.rev)
            glyph: Icons.quote
            checked: SettingsModel.lyricsOpen
            onClicked: SettingsModel.lyricsOpen = !SettingsModel.lyricsOpen
        }
        IconButton {
            tip: I18n.tr("Queue", I18n.rev)
            glyph: Icons.listDetails
            checked: SettingsModel.queueOpen
            onClicked: SettingsModel.queueOpen = !SettingsModel.queueOpen
        }
    }

    PlayerLcd {
        anchors.left: transport.right
        anchors.leftMargin: Metrics.gap16
        anchors.right: actions.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        showTimes: bar.width >= Metrics.playerNoTimes
    }
}
