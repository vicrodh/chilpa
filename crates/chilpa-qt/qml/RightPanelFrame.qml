import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "Patterns"

// The floating right panel over its shadow: the sidebar's twin, from the tab
// bar down to the window's margin, with a grip on its left edge. Its width
// is free in the room an open sidebar has and snaps only to its default, as
// Rust decides it; while the grip is dragged it already shows where it
// would land. It has no collapsed column: it is this wide, or closed.
Item {
    id: frame

    // The width being dragged, or -1 when nothing is.
    property real dragged: -1
    property real grabbed: 0
    property int kept: Number(SettingsModel.kept("ui.rightPanel.width")) || Metrics.rightPanel
    readonly property int shown: SettingsModel.panelWidthAt(dragged >= 0 ? dragged : kept, Window.width)

    RectangularShadow {
        anchors.fill: parent
        radius: panel.radius
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowSubtle
    }

    RightPanel {
        id: panel
        mediaContent: MediaListeningPanel { document: JSON.parse(Sources.snapshot || "{}"); onClosed: SettingsModel.lyricsOpen = false }
        anchors.fill: parent
    }

    ResizeHandle {
        x: -width / 2 // derived: centered on the edge
        height: parent.height
        onStarted: frame.grabbed = frame.shown
        // Dragged to the left it grows.
        onMoved: (delta) => frame.dragged = frame.grabbed - delta
        onReleased: {
            if (frame.dragged >= 0) {
                frame.kept = frame.shown
                SettingsModel.keep("ui.rightPanel.width", String(frame.kept))
            }
            frame.dragged = -1
        }
    }
}
