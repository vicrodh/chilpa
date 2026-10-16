// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import QtQuick.Window
import app.chilpa.graphics
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

LineBedItem {
    id: root

    mirrorVertically: Qt.platform.os === "windows" ? false
        : GraphicsInfo.api !== GraphicsInfo.OpenGL
          && GraphicsInfo.api !== GraphicsInfo.Metal

    primary: Ambient.primary
    accent: Ambient.accent

    function pulseTick(h) {
        if (h !== null)
            heights = h
        update()
    }
}
