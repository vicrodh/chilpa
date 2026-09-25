import QtQuick
import com.blitzfc.chilpa
import "Theme"
import "Primitives"
import "Patterns"

// The window (design/02): the floating sidebar, the tab bar, the screen of
// the active tab and the floating player over it.
Window {
    id: window

    readonly property string sidebarMode: SettingsModel.sidebarMode
    readonly property int sidebarWidth: sidebarMode === "closed" ? 0
        : sidebarMode === "collapsed" ? Metrics.sidebarCollapsed
        : SettingsModel.sidebarWidth > 0 ? SettingsModel.sidebarWidth : Metrics.sidebarWidth
    readonly property int contentLeft: sidebarWidth > 0 ? Metrics.spaceWindow * 2 + sidebarWidth : Metrics.spaceWindow // derived: a margin on each side of the sidebar

    width: Metrics.windowWidth
    height: Metrics.windowHeight
    minimumWidth: Metrics.windowMinWidth
    minimumHeight: Metrics.windowMinHeight
    visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    color: Theme.color.surfaceWindow
    title: "Chilpa"

    Sidebar {
        visible: window.sidebarWidth > 0
        x: Metrics.spaceWindow
        y: Metrics.spaceWindow
        width: window.sidebarWidth
        height: window.height - Metrics.spaceWindow * 2 // derived: a margin above and below
    }

    TabBar {
        x: window.contentLeft
        width: window.width - window.contentLeft - Metrics.spaceWindow
    }

    ScreenHost {
        id: host
        x: window.contentLeft - Metrics.spaceWindow
        y: Metrics.tabBarHeight
        width: window.width - x
        height: window.height - y
    }

    PlayerBar {
        width: Math.min(Metrics.playerMaxWidth, host.width - Metrics.playerInset * 2) // derived: an inset on each side
        x: host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - Metrics.playerBottom
    }

    Tooltip {}

    WindowGrips {
        anchors.fill: parent
        window: window
    }

    DevHooks {
        window: window
    }
}
