// hot-path: one per track row that has something offline; most rows load none.
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A track's offline copy around its menu button, where the row is: QBZ
// marked each track over its offline button. A faint ring while it waits,
// an arc while its file arrives, the accent ring once it is kept and the
// error color when it failed. It takes no room of its own and keeps one
// size in every state, so the row never moves.
Item {
    id: mark
    // Empty, queued, downloading, verifying, removing, ready or failed.
    property string phase: ""
    // How much of the file arrived, or below 0 while that is unknown.
    property real fraction: -1
    property bool inViewport: true
    // The window's shared clock, which turns the arc only while it is seen.
    required property QtObject clock
    readonly property bool working: phase === "downloading" || phase === "verifying" || phase === "removing"

    width: Metrics.offlineRing
    height: Metrics.offlineRing
    visible: phase !== ""

    Rectangle {
        objectName: "offlineRing"
        anchors.fill: parent
        visible: !mark.working
        radius: width / 2 // derived: a circle
        color: "transparent"
        opacity: mark.phase === "queued" ? Metrics.disabledOpacity : 1
        border.width: Metrics.offlineRingStroke
        border.color: mark.phase === "failed" ? Theme.color.statusNegative
            : mark.phase === "ready" ? Theme.color.accent : Theme.color.textTertiary
    }
    Loader {
        objectName: "offlineArc"
        anchors.fill: parent
        active: mark.working && mark.inViewport
        sourceComponent: OfflineArc { clock: mark.clock; fraction: mark.phase === "downloading" ? mark.fraction : -1 }
    }
}
