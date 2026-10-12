import QtQuick
import QtQuick.Effects
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "Patterns"

// The floating sidebar and the grip on its right edge (design/02 2, ADR-0011).
// It starts under the tab bar, where the content and the right panel do.
// While the grip is dragged the sidebar already shows where it would land,
// as Rust decides it: free between its default and 40 % of the window, and
// to the left, collapsed and then closed. Release leaves it there.
Item {
    id: frame

    required property Window window

    // The width being dragged, or -1 when nothing is.
    property real dragged: -1
    property real grabbed: 0

    // Between the planes' top and bottom: the player's layout moves them.
    readonly property int head: Metrics.shellTop
    readonly property int tall: height - head - Metrics.shellBottom // derived: down to the window's margin, or the bar's
    readonly property bool dragging: dragged >= 0
    // The mode while dragging is where the drag would land from where it was.
    property string dragMode: ""
    readonly property string mode: dragging ? dragMode : SettingsModel.sidebarMode
    readonly property int openWidth: dragging ? SettingsModel.sidebarWidthAt(dragged, window.width)
        : SettingsModel.sidebarWidth > 0 ? SettingsModel.sidebarWidth : Metrics.sidebarWidth
    // The width it shows, sliding between its modes the way QBZ's does; a
    // drag follows the pointer, and a mode it snaps to still slides
    // (QBZ's `sidebarSnapping`).
    property bool snapping: false
    property int shown: mode === "closed" ? 0 : mode === "collapsed" ? Metrics.sidebarCollapsed : openWidth
    Behavior on shown { enabled: !frame.dragging || frame.snapping; NumberAnimation { duration: Motion.networkSlide; easing.type: Easing.InOutQuad } }
    readonly property int settled: SettingsModel.sidebarMode === "closed" ? 0
        : SettingsModel.sidebarMode === "collapsed" ? Metrics.sidebarCollapsed
        : SettingsModel.sidebarWidth > 0 ? SettingsModel.sidebarWidth : Metrics.sidebarWidth

    RectangularShadow {
        visible: frame.shown > 0
        x: Metrics.spaceWindow
        y: frame.head
        width: frame.shown
        height: frame.tall
        radius: Metrics.radiusPanel
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowSubtle
    }

    Sidebar {
        visible: frame.shown > 0
        collapsed: frame.mode === "collapsed"
        x: Metrics.spaceWindow
        y: frame.head
        width: frame.shown
        height: frame.tall
    }

    ResizeHandle {
        visible: frame.dragging || SettingsModel.sidebarMode !== "closed"
        x: Metrics.spaceWindow + frame.shown - width / 2 // derived: centered on the edge
        y: frame.head
        height: frame.tall
        // Dragging from the press, as QBZ's `resizing`: the first move
        // already follows the pointer instead of sliding to it.
        onStarted: {
            frame.grabbed = frame.settled
            frame.dragMode = SettingsModel.sidebarMode
            frame.dragged = frame.grabbed
        }
        onMoved: (delta) => {
            const width = frame.grabbed + delta
            const next = SettingsModel.sidebarModeFrom(frame.dragMode, width, frame.window.width)
            frame.snapping = next !== frame.dragMode
            frame.dragged = width
            frame.dragMode = next
        }
        onReleased: {
            if (frame.dragged >= 0)
                SettingsModel.placeSidebar(frame.dragMode, frame.dragged, frame.window.width)
            frame.dragged = -1
            frame.snapping = false
        }
    }
}
