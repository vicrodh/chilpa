// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import QtQuick.Window
import app.chilpa.graphics
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

PlasmaItem {
    id: root

    mirrorVertically: GraphicsInfo.api !== GraphicsInfo.OpenGL
                      && GraphicsInfo.api !== GraphicsInfo.Metal

    function pulseTick(p) {
        time = p.time
        beat = p.beatAbove
        transientAmp = p.transientAmp
        bandsLo = p.bandsLo
        bandsHi = p.bandsHi
        level = p.level
        levelSmooth = p.levelSmooth
        energyLo = p.energyLo
        energyHi = p.energyHi
        primary = p.primary
        secondary = p.secondary
        accent = p.accent
        update()
    }
}
