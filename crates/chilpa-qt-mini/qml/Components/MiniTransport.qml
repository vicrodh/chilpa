import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// The five transport buttons of the mini player (02-mini §1.5): shuffle,
// previous, play or pause, next and repeat, in a row whose sizes the
// footer sets, since it mounts this three times at three scales. Repeat
// takes shuffle's glyph size, as in QBZ. Play has no loading state.
Row {
    id: transport

    property int side: Metrics.button30
    property int iconShuffle: Metrics.iconMd
    property int iconSkip: Metrics.iconNav
    property int iconPlay: Metrics.iconSkip
    property bool lightForeground: false
    // Everything needs a track; play also takes an idle queue with tracks.
    property bool able: true
    property bool playAble: able

    TransportButton {
        glyph: Icons.arrowsShuffle
        iconSize: transport.iconShuffle
        side: transport.side
        active: Player.shuffle
        able: transport.able
        lightForeground: transport.lightForeground
        onClicked: Player.toggleShuffle()
    }
    TransportButton {
        glyph: Icons.playerSkipBackFilled
        filled: true
        iconSize: transport.iconSkip
        side: transport.side
        able: transport.able
        lightForeground: transport.lightForeground
        onClicked: Player.previous()
    }
    TransportButton {
        objectName: "miniPlay"
        glyph: Player.playing && !Player.paused ? Icons.playerPauseFilled : Icons.playerPlayFilled
        filled: true
        iconSize: transport.iconPlay
        side: transport.side
        able: transport.playAble
        lightForeground: transport.lightForeground
        onClicked: Player.toggle()
    }
    TransportButton {
        glyph: Icons.playerSkipForwardFilled
        filled: true
        iconSize: transport.iconSkip
        side: transport.side
        able: transport.able
        lightForeground: transport.lightForeground
        onClicked: Player.next()
    }
    TransportButton {
        glyph: Player.repeat === 2 ? Icons.repeatOnce : Icons.repeat
        iconSize: transport.iconShuffle
        side: transport.side
        active: Player.repeat !== 0
        able: transport.able
        lightForeground: transport.lightForeground
        onClicked: Player.cycleRepeat()
    }
}
