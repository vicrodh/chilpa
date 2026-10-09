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

    readonly property int head: Metrics.tabBarHeight
    readonly property int tall: height - head - Metrics.spaceWindow // derived: down to the window's margin
    readonly property bool dragging: dragged >= 0
    // The mode while dragging is where the drag would land.
    readonly property string mode: dragging ? SettingsModel.sidebarModeAt(dragged, window.width) : SettingsModel.sidebarMode
    readonly property int openWidth: dragging ? SettingsModel.sidebarWidthAt(dragged, window.width)
        : SettingsModel.sidebarWidth > 0 ? SettingsModel.sidebarWidth : Metrics.sidebarWidth
    // The width it shows, sliding between its modes the way QBZ's does; a
    // drag follows the pointer without the slide.
    property int shown: mode === "closed" ? 0 : mode === "collapsed" ? Metrics.sidebarCollapsed : openWidth
    Behavior on shown { enabled: !frame.dragging; NumberAnimation { duration: Motion.networkSlide; easing.type: Easing.InOutQuad } }
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
        onStarted: frame.grabbed = frame.settled
        onMoved: (delta) => frame.dragged = frame.grabbed + delta
        onReleased: {
            if (frame.dragged >= 0)
                SettingsModel.placeSidebar(frame.dragged, frame.window.width)
            frame.dragged = -1
        }
    }
}
