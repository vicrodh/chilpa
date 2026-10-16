// hot-path: shown in the row that plays.
import QtQuick
import app.chilpa.kit
import "../Theme"

// Three bars in the accent (design/03 PlayingIndicator), or, when
// Appearance asks for it, the moving bars while the track runs. Only a
// shown mark of a running track builds them, so a list holds one at most
// and nothing ticks while it is paused or the option is off (QBZ 2.1.2).
Item {
    id: mark
    // The track runs: neither paused nor stopped.
    property bool running: false
    readonly property bool moving: Prefs.animatedIndicator && running && visible
    width: moving ? Metrics.eqWide : Metrics.playingBox
    height: moving ? Metrics.eqTall : Metrics.playingBox

    Loader {
        active: mark.moving
        anchors.fill: parent
        sourceComponent: PlayingBars {}
    }
    Row {
        visible: !mark.moving
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Metrics.gap2

        Repeater {
            model: [0.67, 0.92, 0.5]

            Rectangle {
                required property real modelData
                anchors.bottom: parent.bottom
                width: Metrics.playingBarWidth
                height: Metrics.playingBox * modelData
                radius: 1
                color: Theme.color.accent
            }
        }
    }
}
