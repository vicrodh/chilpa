import QtQuick
import com.blitzfc.chilpa
import "Theme"
import "Primitives"
import "Patterns"

// The floating sidebar and the grip on its right edge (design/02 2). While
// the grip is dragged the width follows the pointer; on release Rust decides
// where it lands: open at that width, collapsed or closed.
Item {
    id: frame

    required property Window window

    // The width being dragged, or -1 when nothing is.
    property real dragged: -1
    property real grabbed: 0

    readonly property string mode: SettingsModel.sidebarMode
    readonly property int settled: mode === "closed" ? 0
        : mode === "collapsed" ? Metrics.sidebarCollapsed
        : SettingsModel.sidebarWidth > 0 ? SettingsModel.sidebarWidth : Metrics.sidebarWidth
    readonly property int shown: dragged >= 0 ? Math.max(0, Math.round(dragged)) : settled

    Sidebar {
        visible: frame.shown > 0
        x: Metrics.spaceWindow
        y: Metrics.spaceWindow
        width: frame.shown
        height: frame.height - Metrics.spaceWindow * 2 // derived: a margin above and below
    }

    ResizeHandle {
        visible: frame.mode !== "closed"
        x: Metrics.spaceWindow + frame.shown - width / 2 // derived: centered on the edge
        y: Metrics.spaceWindow
        height: frame.height - Metrics.spaceWindow * 2 // derived: a margin above and below
        onMoved: (delta) => {
            if (frame.dragged < 0)
                frame.grabbed = frame.settled
            frame.dragged = frame.grabbed + delta
        }
        onReleased: {
            if (frame.dragged >= 0)
                SettingsModel.placeSidebar(frame.dragged, frame.window.width)
            frame.dragged = -1
        }
    }
}
