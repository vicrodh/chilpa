import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/com/blitzfc/chilpa/controls/qml/Patterns" as Driving
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "Patterns"

// The window (design/02): the floating sidebar, the tab bar, the screen of
// the active tab and the floating player over it.
Window {
    id: window

    readonly property int sidebarWidth: sidebar.shown
    readonly property int contentLeft: sidebarWidth > 0 ? Metrics.spaceWindow * 2 + sidebarWidth : Metrics.spaceWindow // derived: a margin on each side of the sidebar
    // The metadata editor wants the width: the queue and the lyrics step
    // aside while it is open and come back as they were when it closes,
    // unless one of them is opened in the meantime.
    property bool panelAside: false

    width: Metrics.windowWidth
    height: Metrics.windowHeight
    minimumWidth: Metrics.windowMinWidth
    minimumHeight: Metrics.windowMinHeight
    visible: true
    // A Mac's window keeps its frame, and with it the system's controls.
    flags: systemControls.kept ? Qt.Window | Qt.ExpandedClientAreaHint | Qt.NoTitleBarBackgroundHint
        : Qt.Window | Qt.FramelessWindowHint
    color: Theme.color.surfaceWindow
    title: "Chilpa"

    // The screen scrolls under nothing but the player: it is cut at the
    // tab bar, which stays on top of it.
    ScreenHost {
        id: host
        x: window.contentLeft
        y: Metrics.tabBarHeight
        width: (right.visible ? right.x : window.width) - Metrics.spaceWindow - x // derived: the window's margin on the right too
        height: window.height - y
        clip: true
    }

    // The content is a plane like the panels beside it.
    PlaneCorners {
        x: host.x
        y: host.y
        width: host.width
    }

    TabBar {
        x: Metrics.spaceWindow
        width: window.width - Metrics.spaceWindow * 2 // derived: a margin on each side
        controlsRoom: systemControls.room
    }

    SystemControls {
        id: systemControls
        window: window
    }

    RightPanelFrame {
        id: right
        z: 2
        visible: (SettingsModel.queueOpen || SettingsModel.lyricsOpen) && !window.panelAside
        x: window.width - width - Metrics.spaceWindow
        y: Metrics.tabBarHeight
        width: shown
        height: window.height - y - Metrics.spaceWindow
    }

    Connections {
        target: Library
        function onContentRevChanged() { Ratings.refresh() }
        function onRemoteRevChanged() { Ratings.refresh() }
    }

    Connections {
        target: Navigation
        function onScreenChanged() { window.panelAside = Navigation.screen === "metadata" }
    }

    PlayerBar {
        id: player
        z: 3
        panelAside: window.panelAside
        onShowPanel: window.panelAside = false
        width: Math.min(Metrics.playerMaxWidth, host.width - Metrics.playerInset * 2) // derived: an inset on each side
        x: host.x + (host.width - width) / 2 // derived: centered over the content
        y: idle.away ? window.height : window.height - height - Metrics.playerBottom
        Behavior on y { NumberAnimation { duration: Motion.panel; easing.type: Easing.OutCubic } }
    }

    PlayerIdle {
        id: idle
        target: window.contentItem
        holding: player.pointed
    }

    SelectionBar {
        id: selection
        z: 4
        visible: AlbumActions.selectedCount > 0
        x: host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - Metrics.selectionBottom
    }

    Toast {
        z: 5
        x: host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - (selection.visible ? Metrics.toastWithSelectionBottom : Metrics.toastBottom)
    }


    // Above the content and the tab bar, so its grip is never covered.
    SidebarFrame {
        id: sidebar
        z: 2
        anchors.fill: parent
        window: window
    }

    Tooltip {}

    // Browser-style side buttons follow the active tab's own history. On the
    // window's root, beneath everything: no control takes those two buttons,
    // so they arrive here, and nothing laid over the window hides the
    // cursors or the hover of what is under it.
    // no cursor: only the mouse's side buttons
    TapHandler {
        parent: window.contentItem
        acceptedButtons: Qt.BackButton | Qt.ForwardButton
        onTapped: (point, button) => button === Qt.BackButton ? Navigation.back() : Navigation.forward()
    }

    WindowGrips {
        anchors.fill: parent
        window: window
    }

    DevHooks {
        window: window
    }

    Driving.WindowControls { window: window }

    // The folders object watches the scans Settings starts, so it exists from
    // the start and not from the first time its page opens.
    readonly property int folders: LibraryFolders.count

    // Nothing starts with the keyboard: the search field only takes it when
    // it is clicked or asked for.
    Component.onCompleted: {
        contentItem.forceActiveFocus()
        panelAside = Navigation.screen === "metadata"
    }

    // Over everything while the plugins start.
    StartupSplash { anchors.fill: parent; z: 1000 }
}
