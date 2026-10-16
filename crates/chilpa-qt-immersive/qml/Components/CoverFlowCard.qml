import QtQuick
import QtQuick.Effects
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// I keep one decoded cover while its position, scale and dim change.
Item {
    id: card
    property string identity: ""
    property url artwork
    property int row: -1
    property bool history: false
    property bool occupied: false
    property bool live: true
    property bool shaders: true
    property real artSize: Stage.flowArtMin
    property real pitch: artSize * Stage.flowPitch
    property real offset: 0
    property real destination: 0
    readonly property real away: Math.abs(offset)
    readonly property bool ready: tile.ready || artwork.toString() === ""
    signal navigate()

    function move(to, snap) {
        destination = to
        motion.stop()
        if (snap || !live || Motion.reduced) offset = to
        else { motion.to = to; motion.start() }
    }
    onLiveChanged: if (!live) { motion.stop(); offset = destination }
    Connections { target: Motion; function onReducedChanged() { if (Motion.reduced) card.move(card.destination, true) } }
    NumberAnimation { id: motion; target: card; property: "offset"; duration: Stage.flowSlide; easing.type: Easing.InOutCubic }

    width: artSize
    height: artSize
    x: (parent.width - width) / 2 + offset * pitch // derived: the same centered fan pitch for every cover
    y: (parent.height - height) / 2 - Stage.flowLift // derived: centered above the metadata
    scale: Math.max(Stage.flowScaleFloor, 1 - Stage.flowScaleFall * away)
    z: -away
    visible: occupied && away <= Stage.flowPaintReach
    Rectangle {
        visible: card.away < 0.5
        anchors.fill: parent
        anchors.topMargin: Stage.flowShadowDrop
        radius: Stage.flowRound
        color: Theme.color.immersiveShadow
        layer.enabled: card.shaders
        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachArt; blur: 0.875 }
    }
    ArtTile {
        id: tile
        objectName: "flowArtwork"
        anchors.fill: parent
        radius: Stage.flowRound
        edged: false
        source: card.artwork
    }
    Rectangle { anchors.fill: parent; radius: Stage.flowRound; color: Theme.color.immersiveBlack; opacity: Math.min(Stage.flowDimCap, card.away * Stage.flowDimRate) }
    MouseArea {
        anchors.fill: parent
        enabled: card.occupied && Math.abs(card.destination) > 0.5
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: card.navigate()
    }
}
