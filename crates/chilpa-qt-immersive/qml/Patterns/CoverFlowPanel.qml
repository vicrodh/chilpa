import QtQuick
import QtQuick.Window
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Theme"

Item {
    id: panel
    property bool shaders: true
    readonly property real baseArt: Math.max(Stage.flowArtMin, Math.min(panel.height - Stage.flowHeightLess, panel.width * Stage.flowShare, Stage.flowArtMax))
    readonly property bool live: visible && (!Window.window || (Window.window.visibility !== Window.Hidden && Window.window.visibility !== Window.Minimized))
    TrackListModel { id: queueRows; kind: "queue"; filterRev: Player.queueRev + ListenList.revision; playingPath: Player.path }
    TrackListModel { id: historyRows; kind: "history"; filterRev: Player.queueRev + ListenList.revision; playingPath: Player.path }
    Timer { interval: Motion.listenRefresh; repeat: true; running: panel.live; onTriggered: ListenList.refresh() }

    Item {
        anchors.fill: parent
        anchors.topMargin: Stage.flowPadTop
        anchors.bottomMargin: Stage.flowPadBottom
        Column {
            anchors.centerIn: parent
            width: panel.width
            spacing: Stage.flowApart
            CoverFlowDeck {
                width: panel.width
                height: panel.baseArt + Stage.flowStageExtra
                artSize: panel.baseArt
                shaders: panel.shaders
                live: panel.live
                ahead: queueRows
                behind: historyRows
                currentPath: Player.path
                currentArt: Player.art
            }
            TrackMeta { width: panel.width }
        }
    }
}
