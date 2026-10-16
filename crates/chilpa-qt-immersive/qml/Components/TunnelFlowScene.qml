// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import QtQuick.Window
import app.chilpa.graphics
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

TunnelFlowItem {
    id: root

    mirrorVertically: GraphicsInfo.api !== GraphicsInfo.OpenGL
                      && GraphicsInfo.api !== GraphicsInfo.Metal

    opacity: 0
    Component.onCompleted: fadeIn.start()
    NumberAnimation {
        id: fadeIn
        target: root
        property: "opacity"
        from: 0
        to: 1
        duration: Motion.reduced ? 0 : Stage.sceneFade
    }

    property var smooth: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    property real tfPhase: 0
    property real kickPulse: 0
    property real prevBass: 0
    property real prevHigh: 0

    property var paletteStash: JSON.parse(Ambient.tunnelPaletteJson)

    Connections {
        target: Ambient
        function onTunnelPaletteJsonChanged() {
            try {
                root.paletteStash = JSON.parse(Ambient.tunnelPaletteJson)
            } catch (e) {
            }
        }
    }

    function pulseTick(timeS, bars) {
        var s = root.smooth
        if (bars !== null && bars.length >= 16) {
            for (var i = 0; i < 16; i++)
                s[i] = s[i] * 0.72 + bars[i] * 0.28
            var bass = (s[0] + s[1] + s[2] + s[3]) / 4
            var high = (s[10] + s[11] + s[12] + s[13] + s[14] + s[15]) / 6
            var bassDelta = bass - root.prevBass
            var highDelta = high - root.prevHigh
            root.prevBass = bass
            root.prevHigh = high
            if (bassDelta > 0.05 || highDelta > 0.05 || bass > 0.7) {
                root.kickPulse = Math.max(0, Math.min(1,
                    0.24 + bass * 0.52 + high * 0.34 + Math.max(0, bassDelta) * 1.8))
            }
        }
        var bass = (s[0] + s[1] + s[2] + s[3]) / 4
        var mid = (s[4] + s[5] + s[6] + s[7] + s[8] + s[9]) / 6
        var high = (s[10] + s[11] + s[12] + s[13] + s[14] + s[15]) / 6

        var pal = root.paletteStash
        if (pal !== null && pal.length >= 4) {
            palette0 = pal[0]
            palette1 = pal[1]
            palette2 = pal[2]
            palette3 = pal[3]
        }

        time = timeS
        phase = root.tfPhase
        root.bass = bass
        root.mid = mid
        root.high = high
        kick = root.kickPulse

        root.tfPhase += 0.012 + bass * 0.026 + high * 0.014 + root.kickPulse * 0.018
        root.kickPulse *= 0.9
        update()
    }
}
