import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

// Focus 0, Album Reactive (03-immersive §1.4): the cover breathes with the
// music. The five energy bands come through one StreamSettle; the mean of
// the five scales the cover from 1 to 1.25 and lights the halo, the mean
// of the two bass bands spreads it. The scalars apply at the pulse rate
// with no animation of their own: the window presents once per pulse.
// The cover grows from its center by width and height; x and y are plain
// bindings, so it never drifts.
Item {
    id: panel
    property bool shaders: true
    readonly property bool live: visible && Immersive.open

    StreamSettle {
        id: energy
        target: Visualizer.energy
        live: panel.live
    }
    readonly property real globalEnergy: energy.cur.length < 5 ? 0 : (energy.at(0) + energy.at(1) + energy.at(2) + energy.at(3) + energy.at(4)) / 5
    readonly property real bassEnergy: energy.cur.length < 2 ? 0 : (energy.at(0) + energy.at(1)) / 2
    readonly property real artScale: 1 + globalEnergy * Stage.reactiveScaleGain
    readonly property real glowSpread: Stage.glowBase + bassEnergy * Stage.glowGain
    readonly property real glowOpacity: Math.min(Stage.glowOpacityBase + globalEnergy * Stage.glowOpacityGain, 1)

    // The cover is never drawn larger than it is: its own width caps the
    // slot, and the breathing still grows it for a moment.
    Image { id: probe; visible: false; asynchronous: true; source: panel.visible ? Player.art : "" }
    readonly property real ownWidth: probe.status === Image.Ready ? probe.implicitWidth : 0
    readonly property real baseArt: Math.max(Stage.reactiveArtMin, Math.min((panel.height - Stage.reactiveHeightLess) / Stage.reactiveArtDivisor, panel.width * Stage.reactiveArtShare, Stage.reactiveArtMax, ownWidth))
    readonly property real artSize: baseArt * artScale

    Item {
        anchors.fill: parent
        anchors.topMargin: Stage.reactivePadTop
        anchors.bottomMargin: Stage.reactivePadBottom
        Column {
            anchors.centerIn: parent
            width: panel.width
            spacing: Stage.reactiveApart
            Item {
                width: panel.width
                height: panel.baseArt * Stage.reactiveArtDivisor
                Rectangle {
                    width: panel.artSize
                    height: panel.artSize
                    x: (parent.width - width) / 2 // derived: centered
                    y: (parent.height - height) / 2 + Stage.artShadowDrop // derived: centered, dropped
                    radius: Stage.artRound
                    color: Theme.color.immersiveShadowArt
                    layer.enabled: panel.shaders
                    layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachArt; blur: 1 }
                }
                Rectangle {
                    width: panel.artSize
                    height: panel.artSize
                    x: (parent.width - width) / 2 // derived: centered
                    y: (parent.height - height) / 2 // derived: centered
                    radius: Stage.artRound
                    color: Ambient.glow
                    opacity: panel.glowOpacity
                    layer.enabled: panel.shaders
                    layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReach; blur: Math.min(1, panel.glowSpread / Stage.glowBlurDivisor) }
                }
                ArtTile {
                    width: panel.artSize
                    height: panel.artSize
                    x: (parent.width - width) / 2 // derived: centered
                    y: (parent.height - height) / 2 // derived: centered
                    radius: Stage.artRound
                    edged: false
                    source: Player.art
                }
            }
            TrackMeta { width: panel.width }
        }
    }
}
