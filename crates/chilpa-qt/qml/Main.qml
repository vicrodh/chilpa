import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.controls
import app.chilpa.folders
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Driving
import "qrc:/qt/qml/app/chilpa/ambient/qml/Patterns" as Backdrop
import "qrc:/qt/qml/app/chilpa/mini/qml/Screens" as Mini
import "qrc:/qt/qml/app/chilpa/immersive/qml/Screens" as Overlay
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
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

    // As big as it was left, and maximized if it was (design/04 Window).
    width: WindowPlace.width
    height: WindowPlace.height
    minimumWidth: WindowPlace.smallestWidth
    minimumHeight: WindowPlace.smallestHeight
    visibility: WindowPlace.maximized ? Window.Maximized : Window.Windowed
    // A Mac's window keeps its frame, and with it the system's controls.
    flags: systemControls.kept ? Qt.Window | Qt.ExpandedClientAreaHint | Qt.NoTitleBarBackgroundHint
        : Qt.Window | Qt.FramelessWindowHint
    color: Theme.color.surfaceWindow
    title: "Chilpa"

    // The dynamic background, under everything (design/01 1.6).
    Backdrop.AppBackground {
        anchors.fill: parent
        plane: Qt.rect(host.x, host.y, host.width, host.height)
    }

    // The screen scrolls under nothing but the player: it is cut at the
    // tab bar, which stays on top of it.
    ScreenHost {
        id: host
        x: window.contentLeft
        y: Metrics.tabBarHeight
        width: (right.visible ? right.x : window.width) - Metrics.spaceWindow - x // derived: the window's margin on the right too
        // Under the dynamic background the plane is a card that ends at the
        // window's margin, so the screens end there too.
        height: window.height - y - (Theme.ambient ? Metrics.spaceWindow : 0)
        clip: true
    }

    // The content is a plane like the panels beside it. Under the dynamic
    // background the field shows through its corners instead.
    PlaneCorners {
        visible: !Theme.ambient
        x: host.x
        y: host.y
        width: host.width
    }

    TabBar {
        x: Metrics.spaceWindow
        width: window.width - Metrics.spaceWindow * 2 // derived: a margin on each side
        controlsRoom: systemControls.room
        onQuitAsked: driving.requestQuit()
    }

    SystemControls {
        id: systemControls
        window: window
    }

    RightPanelFrame {
        id: right
        z: 2
        open: (SettingsModel.queueOpen || SettingsModel.lyricsOpen) && !window.panelAside
        x: window.width - width - Metrics.spaceWindow
        y: Metrics.tabBarHeight
        height: window.height - y - Metrics.spaceWindow
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
        // The plane's width less the window's gap on each side, and its
        // bottom that gap above the plane's (the owner, 2026-10-08 night).
        width: host.width - Metrics.spaceWindow * 2 // derived: the gap on each side
        x: host.x + Metrics.spaceWindow
        y: idle.away ? window.height : host.y + host.height - height - Metrics.spaceWindow
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

    WindowGrips {
        anchors.fill: parent
        window: window
    }

    DevHooks {
        window: window
    }

    Driving.WindowControls { id: driving; window: window }
    Mini.MiniHost { window: window; controls: driving }
    Overlay.ImmersiveView { anchors.fill: parent; z: 6; window: window; controls: driving }

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
    // A press outside the field that has the keyboard, or another view, takes it.
    FocusRelease { key: [Navigation.active, Navigation.screen, Navigation.routeId, Navigation.routeKey].join(" ") }
}
