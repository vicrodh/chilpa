import QtQuick
import QtQuick.Effects
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

// Focus 2, Coverflow (03-immersive §1.4): the covers of what played and
// what comes, fanned flat about the one that plays, each at
// max(0.5, 1 - 0.18 |offset|) of the base and dimmed min(0.55, 0.18
// |offset|), pitched 0.42 of the base apart. Only the seven nearest paint
// and only the thirteen nearest exist. A click on a side cover plays it.
// On a track change the fan slides one slot into place over 320 ms, the
// only motion here.
Item {
    id: panel
    property bool shaders: true
    readonly property real baseArt: Math.max(Stage.flowArtMin, Math.min(panel.height - Stage.flowHeightLess, panel.width * Stage.flowShare, Stage.flowArtMax))
    readonly property real pitch: baseArt * Stage.flowPitch

    // The rows ahead, the one playing first; and the rows behind, the most
    // recent first.
    TrackListModel { id: ahead; kind: "queue"; filterRev: Player.queueRev + ListenList.revision; playingPath: Player.path }
    TrackListModel { id: behind; kind: "history"; filterRev: Player.queueRev + ListenList.revision; playingPath: Player.path }
    Timer { interval: Motion.listenRefresh; repeat: true; running: panel.visible; onTriggered: { ahead.refresh(); behind.refresh() } }

    // The slide: a new track lands from the right, so every cover starts
    // one slot to the right and eases into place.
    property real slide: 0
    property bool snapping: false
    Behavior on slide { enabled: !panel.snapping; NumberAnimation { duration: Motion.reduced ? 0 : Stage.flowSlide; easing.type: Easing.InOutQuad } }
    Connections {
        target: Player
        function onPathChanged() {
            if (!panel.visible)
                return
            panel.snapping = true
            panel.slide = 1
            panel.snapping = false
            panel.slide = 0
        }
    }

    component CoverCard: Item {
        id: card
        property url art
        property real offset: 0
        property bool interactive: false
        signal navigate()
        readonly property real away: Math.abs(offset)
        readonly property real scale: Math.max(Stage.flowScaleFloor, 1 - Stage.flowScaleFall * away)
        readonly property real slot: panel.baseArt * scale
        readonly property real dim: Math.min(Stage.flowDimCap, away * Stage.flowDimRate)
        visible: away <= Stage.flowPaintReach
        width: slot
        height: slot
        x: stage.width / 2 + offset * panel.pitch - slot / 2 // derived: centered on its pitch
        y: (stage.height - slot) / 2 - Stage.flowLift // derived: centered, lifted
        Rectangle {
            visible: card.away < 0.5
            anchors.fill: parent
            anchors.topMargin: Stage.flowShadowDrop
            radius: Stage.flowRound
            color: Theme.color.immersiveShadow
            layer.enabled: panel.shaders
            layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachArt; blur: 0.875 }
        }
        Rectangle { visible: card.art.toString() === ""; anchors.fill: parent; radius: Stage.flowRound; color: Theme.color.immersivePlaceholder }
        ArtTile { visible: card.art.toString() !== ""; anchors.fill: parent; radius: Stage.flowRound; edged: false; source: card.art }
        Rectangle { anchors.fill: parent; radius: Stage.flowRound; color: Theme.color.immersiveBlack; opacity: card.dim }
        MouseArea {
            anchors.fill: parent
            enabled: card.interactive && card.away > 0.5
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: card.navigate()
        }
    }

    Item {
        anchors.fill: parent
        anchors.topMargin: Stage.flowPadTop
        anchors.bottomMargin: Stage.flowPadBottom
        Column {
            anchors.centerIn: parent
            width: panel.width
            spacing: Stage.flowApart
            Item {
                id: stage
                width: panel.width
                height: panel.baseArt + Stage.flowStageExtra
                // The nearer a cover, the higher it stacks; the one playing
                // is on top of all.
                Repeater {
                    model: behind
                    Item {
                        id: past
                        required property int index
                        required property url art
                        readonly property real offset: -(index + 1) + panel.slide
                        z: -(index + 1)
                        Loader {
                            active: Math.abs(past.offset) < Stage.flowLiveReach
                            sourceComponent: CoverCard { art: past.art; offset: past.offset; interactive: true; onNavigate: behind.play(past.index) }
                        }
                    }
                }
                Repeater {
                    model: ahead
                    Item {
                        id: next
                        required property int index
                        required property url art
                        readonly property real offset: index + panel.slide
                        z: -index
                        Loader {
                            active: next.index > 0 && Math.abs(next.offset) < Stage.flowLiveReach
                            sourceComponent: CoverCard { art: next.art; offset: next.offset; interactive: true; onNavigate: ahead.play(next.index) }
                        }
                    }
                }
                CoverCard { z: 1; art: Player.art; offset: panel.slide; interactive: false }
            }
            TrackMeta { width: panel.width }
        }
    }
}
