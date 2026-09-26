import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import "Theme"
import "Primitives"
import "Patterns"

// The window (design/02): the floating sidebar, the tab bar, the screen of
// the active tab and the floating player over it.
Window {
    id: window

    readonly property int sidebarWidth: sidebar.shown
    readonly property int contentLeft: sidebarWidth > 0 ? Metrics.spaceWindow * 2 + sidebarWidth : Metrics.spaceWindow // derived: a margin on each side of the sidebar

    width: Metrics.windowWidth
    height: Metrics.windowHeight
    minimumWidth: Metrics.windowMinWidth
    minimumHeight: Metrics.windowMinHeight
    visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    color: Theme.color.surfaceWindow
    title: "Chilpa"

    // The screen scrolls under nothing but the player: it is cut at the
    // tab bar, which stays on top of it.
    ScreenHost {
        id: host
        x: window.contentLeft
        y: Metrics.tabBarHeight
        width: (right.visible ? right.x - Metrics.spaceWindow : window.width) - x
        height: window.height - y
        clip: true
    }

    TabBar {
        x: window.contentLeft
        width: window.width - window.contentLeft - Metrics.spaceWindow
    }

    RightPanel {
        id: right
        z: 2
        visible: SettingsModel.queueOpen || SettingsModel.lyricsOpen
        x: window.width - width - Metrics.spaceWindow
        y: Metrics.tabBarHeight
        width: Metrics.rightPanel
        height: window.height - y - Metrics.spaceWindow
    }

    RectangularShadow {
        anchors.fill: right
        visible: right.visible
        radius: right.radius
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowSubtle
        z: 1
    }

    PlayerBar {
        z: 3
        width: Math.min(Metrics.playerMaxWidth, host.width - Metrics.playerInset * 2) // derived: an inset on each side
        x: host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - Metrics.playerBottom
    }

    SelectionBar {
        z: 4
        visible: AlbumActions.selectedCount > 0
        x: host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - Metrics.selectionBottom
    }

    // Above the content and the tab bar, so its grip is never covered.
    SidebarFrame {
        id: sidebar
        z: 2
        anchors.fill: parent
        window: window
    }

    Tooltip {}

    WindowGrips {
        anchors.fill: parent
        window: window
    }

    DevHooks {
        window: window
    }

    Shortcuts {}

    // The folders object watches the scans Settings starts, so it exists from
    // the start and not from the first time its page opens.
    readonly property int folders: LibraryFolders.count

    // Nothing starts with the keyboard: the search field only takes it when
    // it is clicked or asked for.
    Component.onCompleted: contentItem.forceActiveFocus()
}
