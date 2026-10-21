// hot-path: shown in the row that plays.
import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// The playing row's mark: Lucide's circle-play, circle-pause under the
// pointer while the track runs, and a click asks to pause or resume. When
// Appearance asks for it, the moving bars take its place while the track
// runs. Only a shown mark builds either, so a list holds one at most and
// nothing ticks while it is paused or the option is off (QBZ 2.1.2).
Item {
    id: mark
    // The track runs: neither paused nor stopped.
    property bool running: false
    property bool sceneAnimating: false
    readonly property bool moving: Prefs.animatedIndicator && running && visible
    signal toggled()
    width: moving ? Metrics.eqWide : Metrics.iconMd
    height: moving ? Metrics.eqTall : Metrics.iconMd

    Loader {
        active: mark.moving
        anchors.fill: parent
        sourceComponent: PlayingBars { sceneAnimating: mark.sceneAnimating }
    }
    Loader {
        active: !mark.moving && mark.visible
        anchors.fill: parent
        sourceComponent: Icon {
            objectName: "playingMarkStill"
            glyph: mark.running && hover.hovered ? Lucide.circlePause : Lucide.circlePlay
            iconFamily: Lucide.font
            size: Metrics.iconMd
            color: Theme.color.accent
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: mark.toggled() }
        }
    }
}
