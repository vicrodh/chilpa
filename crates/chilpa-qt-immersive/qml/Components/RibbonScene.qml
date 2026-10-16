// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import QtQuick.Window
import app.chilpa.graphics
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

RibbonItem {
    id: root

    mirrorVertically: GraphicsInfo.api !== GraphicsInfo.OpenGL
                      && GraphicsInfo.api !== GraphicsInfo.Metal

    function pulseTick(f, peak) {
        if (f !== null)
            frame = f
        energyHi = peak
        update()
    }
}
