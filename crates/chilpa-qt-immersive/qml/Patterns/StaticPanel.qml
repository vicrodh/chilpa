import QtQuick
import QtQuick.Effects
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

// Focus 1, Static (03-immersive §1.4): the cover at rest, as large as
// max(180, min(height - 364, width * 0.62, 640, its own width)), over its
// shadow, with the track's block 20 below.
Item {
    id: panel
    property bool shaders: true
    Image { id: probe; visible: false; asynchronous: true; source: panel.visible ? Player.art : "" }
    readonly property real ownWidth: probe.status === Image.Ready ? probe.implicitWidth : 0
    readonly property real baseArt: Math.max(Stage.reactiveArtMin, Math.min(panel.height - Stage.staticHeightLess, panel.width * Stage.reactiveArtShare, Stage.staticArtMax, ownWidth))

    Item {
        anchors.fill: parent
        anchors.topMargin: Stage.reactivePadTop
        anchors.bottomMargin: Stage.reactivePadBottom
        Column {
            anchors.centerIn: parent
            width: panel.width
            spacing: Stage.staticApart
            Item {
                width: panel.width
                height: panel.baseArt
                Rectangle {
                    width: panel.baseArt
                    height: panel.baseArt
                    x: (parent.width - width) / 2 // derived: centered
                    y: Stage.artShadowDrop
                    radius: Stage.artRound
                    color: Theme.color.immersiveShadowArt
                    layer.enabled: panel.shaders
                    layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachArt; blur: 1 }
                }
                ArtTile {
                    width: panel.baseArt
                    height: panel.baseArt
                    x: (parent.width - width) / 2 // derived: centered
                    radius: Stage.artRound
                    edged: false
                    source: Player.art
                }
            }
            TrackMeta { width: panel.width }
        }
    }
}
