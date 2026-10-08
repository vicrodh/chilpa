import QtQuick
import app.chilpa
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Theme"

// The mini player's control strip (02-mini §1.4), in three layouts by
// `mode`: 0 full, 1 compact, 2 micro, numbered the other way round from
// the surfaces, as QBZ's is; the shell turns one into the other. In micro
// the footer IS the card. It rounds its own corners: a Rectangle's clip
// does not cut its children to its radius, and an opaque footer anchored
// to the card's bottom would paint square corners over the round ones.
//
// The seek bar shows the position asked for until the player lands on it,
// or a watchdog gives up, so a click never snaps back under the pointer
// while the engine seeks.
Rectangle {
    id: footer

    property int mode: 0
    property var hostWindow: null
    property bool windowHovered: false
    property bool backgroundActive: false
    property real cardRadius: 0

    readonly property int tier: mode === 1 ? 1 : 0
    readonly property bool micro: mode === 2
    readonly property bool lightForeground: backgroundActive && Theme.dark
    readonly property bool hasTrack: Player.path !== ""
    readonly property bool canPlay: hasTrack || Mini.queueHasTarget
    readonly property real played: Player.duration > 0 ? Player.position / Player.duration : 0

    // The pending seek: -1 is none, since 0 is a position.
    property real dragPreview: -1
    property real pendingSeek: -1
    property bool dragging: false
    readonly property real shownProgress: dragging && dragPreview >= 0 ? dragPreview : pendingSeek >= 0 ? pendingSeek : played

    Connections {
        target: Player
        function onPositionChanged() {
            if (footer.pendingSeek >= 0 && Math.abs(footer.played - footer.pendingSeek) < Card.seekConverge) {
                footer.pendingSeek = -1
                watchdog.stop()
            }
        }
    }
    Timer {
        id: watchdog
        interval: Card.seekWatchdog
        onTriggered: footer.pendingSeek = -1
    }

    function previewSeek(fraction) {
        dragging = true
        dragPreview = fraction
    }
    function commitSeek(fraction) {
        dragging = false
        dragPreview = -1
        pendingSeek = fraction
        Player.seek(fraction * Player.duration)
        watchdog.restart()
    }

    color: backgroundActive ? Theme.color.miniFooter : Theme.color.surfacePanel
    topLeftRadius: micro ? cardRadius : 0
    topRightRadius: micro ? cardRadius : 0
    bottomLeftRadius: cardRadius
    bottomRightRadius: cardRadius

    Loader {
        active: !footer.micro
        anchors.fill: parent
        anchors.leftMargin: Card.padSide[footer.tier]
        anchors.rightMargin: Card.padSide[footer.tier]
        anchors.bottomMargin: Card.padBottom[footer.tier]
        sourceComponent: MiniFooterBand {
            tier: footer.tier
            lightForeground: footer.lightForeground
            progress: footer.shownProgress
            hostWindow: footer.hostWindow
            windowHovered: footer.windowHovered
            hasTrack: footer.hasTrack
            canPlay: footer.canPlay
            onPreview: (fraction) => footer.previewSeek(fraction)
            onCommit: (fraction) => footer.commitSeek(fraction)
        }
    }
    Loader {
        active: footer.micro
        anchors.fill: parent
        anchors.leftMargin: Card.microPad
        anchors.rightMargin: Card.microPad
        anchors.topMargin: Card.microTop
        sourceComponent: MiniFooterMicro {
            lightForeground: footer.lightForeground
            progress: footer.shownProgress
            hostWindow: footer.hostWindow
            windowHovered: footer.windowHovered
            hasTrack: footer.hasTrack
            canPlay: footer.canPlay
            onPreview: (fraction) => footer.previewSeek(fraction)
            onCommit: (fraction) => footer.commitSeek(fraction)
        }
    }
}
