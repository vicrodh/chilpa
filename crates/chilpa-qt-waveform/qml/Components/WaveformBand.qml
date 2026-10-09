import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The band the player bar grows by when it stands tall: the elapsed time,
// the waveform seek and the time left, side by side (the owner,
// 2026-10-09). The bar's own row stays where it was underneath.
Item {
    id: band

    Text {
        id: elapsed
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        visible: Player.playing
        text: Player.elapsed
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    Text {
        id: remaining
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: Player.playing && !Player.live
        text: Player.remaining
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    WaveformSeek {
        objectName: "playerWaveform"
        visible: !Player.live
        anchors.left: elapsed.visible ? elapsed.right : parent.left
        anchors.right: remaining.visible ? remaining.left : parent.right
        anchors.leftMargin: Metrics.gap10
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        height: band.height - Metrics.gap8 // derived: the band less its air
        position: Player.position
        duration: Player.duration
        bubbleText: Player.clock(hoverSeconds)
        loopStart: Controls.loopState > 0 ? Controls.loopStart : -1
        loopEnd: Controls.loopState > 1 ? Controls.loopEnd : -1
        onSeek: (seconds) => Player.seek(seconds)
    }
}
