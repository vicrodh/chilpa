import QtQuick
import app.chilpa
import app.chilpa.folders
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
    property var router: null
    readonly property int folders: LibraryFolders.count
    function openSource(action, id, text) { content.openSource(action, id, text) }
    function deferNavigation(action) { content.deferNavigation(action) }

    property var drag: null
    property real grabbed: 0
    property bool snapping: false
    readonly property bool dragging: drag !== null
    readonly property bool moving: dragging || slide.running
    readonly property int head: Metrics.shellTop
    readonly property int tall: height - head - Metrics.shellBottom
    readonly property int kept: SettingsModel.sidebarWidth > 0 ? SettingsModel.sidebarWidth : Metrics.sidebarWidth
    readonly property string mode: dragging ? drag.mode : SettingsModel.sidebarMode
    readonly property int targetWidth: mode === "closed" ? 0 : mode === "collapsed" ? Metrics.sidebarCollapsed
        : dragging ? drag.width : kept

    property real shown: targetWidth
    Behavior on shown {
        enabled: !frame.dragging || frame.snapping
        NumberAnimation { id: slide; duration: Motion.networkSlide; easing.type: Easing.InOutQuad }
    }

    Shadow {
        visible: frame.shown > 0 && !frame.moving
        x: viewport.x
        y: viewport.y
        width: viewport.width
        height: viewport.height
        radius: Metrics.radiusPanel
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowSubtle
    }

    Item {
        id: viewport
        x: Metrics.spaceWindow
        y: frame.head
        width: frame.shown
        height: frame.tall
        visible: width > 0
        clip: frame.moving

        Rectangle {
            anchors.fill: parent
            visible: frame.dragging && viewport.width > content.width
            radius: Metrics.radiusPanel
            color: content.color
        }

        Sidebar {
            id: content
            objectName: "sidebarContent"
            router: frame.router
            // Keep the current layout on close. In an open drag only the
            // viewport width changes until the grip is released.
            collapsed: frame.mode === "closed" ? frame.lastMode === "collapsed" : frame.mode === "collapsed"
            width: collapsed ? Metrics.sidebarCollapsed : frame.kept
            height: viewport.height
            x: frame.dragging ? 0 : Math.min(0, viewport.width - width)
        }
    }

    property string lastMode: "open"
    onModeChanged: if (mode !== "closed") lastMode = mode

    ResizeHandle {
        visible: frame.dragging || SettingsModel.sidebarMode !== "closed"
        x: Metrics.spaceWindow + frame.shown - width / 2 // derived: centered on the edge
        y: frame.head
        height: frame.tall
        // Dragging from the press, as QBZ's `resizing`: the first move
        // already follows the pointer instead of sliding to it.
        onStarted: {
            frame.grabbed = frame.shown
            frame.drag = { mode: SettingsModel.sidebarMode, width: frame.kept }
        }
        onMoved: (delta) => {
            const raw = frame.grabbed + delta
            const next = SettingsModel.sidebarModeFrom(frame.mode, raw, frame.window.width)
            frame.snapping = next !== frame.mode || (frame.snapping && (next === "closed" || next === "collapsed"))
            frame.drag = { mode: next, width: SettingsModel.sidebarWidthAt(raw, frame.window.width), raw: raw }
        }
        onReleased: {
            if (frame.drag && frame.drag.raw !== undefined)
                SettingsModel.placeSidebar(frame.mode, frame.drag.raw, frame.window.width)
            frame.drag = null
            frame.snapping = false
        }
        onCanceled: {
            frame.drag = null
            frame.snapping = false
        }
    }
}
