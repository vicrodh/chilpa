import QtQuick
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Driving
import "qrc:/qt/qml/app/chilpa/ambient/qml/Patterns" as Backdrop
import "qrc:/qt/qml/app/chilpa/mini/qml/Screens" as Mini
import "qrc:/qt/qml/app/chilpa/immersive/qml/Screens" as Overlay
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
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
    property alias panelAside: host.panelAside
    function deferNavigation(action, ...args) { sidebar.deferNavigation(function() { action.apply(null, args) }) }

    // As big as it was left, and maximized if it was (design/04 Window).
    width: WindowPlace.width
    height: WindowPlace.height
    minimumWidth: DisplayMode.customPlayer ? Math.max(Metrics.displayMinWidth, DisplayMode.provider.minimumWidth || 0) : WindowPlace.smallestWidth
    minimumHeight: DisplayMode.active ? Math.max(Metrics.displayMinHeight, DisplayMode.provider ? DisplayMode.provider.minimumHeight || 0 : 0) : WindowPlace.smallestHeight
    visibility: WindowPlace.maximized ? Window.Maximized : Window.Windowed
    // A Mac's window keeps its frame, and with it the system's controls.
    flags: systemControls.kept ? Qt.Window | Qt.ExpandedClientAreaHint | Qt.NoTitleBarBackgroundHint
        : Qt.Window | Qt.FramelessWindowHint
    color: Theme.color.surfaceWindow
    // On a Mac the title bar is under the tab bar, and a title there would
    // sit beside the system's controls. Hiding its text from AppKit alone
    // does not keep it away, so there is none to draw, as in QBZ.
    title: systemControls.kept ? "" : "Chilpa"

    // The dynamic background, under everything (design/01 1.6).
    Backdrop.AppBackground {
        visible: !DisplayMode.active || !DisplayMode.provider || DisplayMode.provider.ambient !== false
        anchors.fill: parent
        plane: DisplayMode.active ? Qt.rect(0, 0, window.width, window.height) : Qt.rect(host.x, host.y, host.width, host.height)
    }

    // The screen scrolls under nothing but the player: it is cut at the
    // tab bar, which stays on top of it.
    ScreenHost {
        id: host
        resizing: sidebar.dragging || right.dragged >= 0
        x: window.contentLeft
        y: plan.planeTop
        width: resizing && heldWidth >= 0 ? heldWidth : (right.visible ? right.x : window.width) - Metrics.spaceWindow - x
        // Under the dynamic background the plane is a card that ends at the
        // window's margin, so the screens end there too.
        height: window.height - y - plan.contentBottom
        clip: true
    }

    // The content is a plane like the panels beside it. Under the dynamic
    // background the field shows through its corners instead.
    PlaneCorners {
        visible: !DisplayMode.active && !Theme.ambient
        x: host.x
        y: host.y
        width: host.width
    }

    TabBar {
        openSource: sidebar.openSource
        x: Metrics.spaceWindow
        width: window.width - Metrics.spaceWindow - Metrics.spaceWindow // derived: a margin on each side
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
        open: !DisplayMode.active && (SettingsModel.queueOpen || SettingsModel.lyricsOpen) && !window.panelAside
        x: window.width - width - Metrics.spaceWindow
        y: plan.planeTop
        height: window.height - y - plan.planeBottom
    }


    PlayerBar {
        id: player
        z: 3
        panelAside: window.panelAside
        onShowPanel: window.panelAside = false
        // Floating, the plane's width less the window's gap on each side
        // (the owner, 2026-10-08 night); docked, the window's (PlayerPlan).
        visible: !DisplayMode.customPlayer
        width: DisplayMode.active ? window.width - Metrics.spaceWindow - Metrics.spaceWindow : plan.barWidth
        x: DisplayMode.active ? Metrics.spaceWindow : plan.barX
        y: DisplayMode.active ? window.height - height - Metrics.spaceWindow : plan.barY
        Behavior on y { NumberAnimation { duration: Motion.panel; easing.type: Easing.OutCubic } }
    }
    Driving.PlayerPlan { id: plan; window: window; content: host; tall: player.implicitHeight; away: idle.away }

    PlayerIdle {
        id: idle
        target: window.contentItem
        holding: player.pointed
    }

    SelectionBar {
        z: 4
        visible: !DisplayMode.active && AlbumActions.selectedCount > 0
        x: DisplayMode.active ? (window.width - width) / 2 : host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - Metrics.selectionBottom
    }

    Toast {
        z: 5
        x: DisplayMode.active ? (window.width - width) / 2 : host.x + (host.width - width) / 2 // derived: centered over the content
        y: window.height - height - (!DisplayMode.active && AlbumActions.selectedCount > 0 ? Metrics.toastWithSelectionBottom : Metrics.toastBottom)
    }

    // Above the content and the tab bar, so its grip is never covered.
    SidebarFrame {
        id: sidebar
        router: host
        z: 2
        anchors.fill: parent
        window: window
    }

    Tooltip { watching: window.contentItem }
    QueueProgress { anchors.right: parent.right; anchors.rightMargin: Metrics.spaceWindow; y: plan.planeTop + Metrics.gap8; z: 7 }

    WindowGrips { anchors.fill: parent; window: window }
    DevHooks { window: window }

    Driving.WindowControls { id: driving; window: window }
    Mini.MiniHost { window: window; controls: driving; beforeEnter:() => { if(DisplayMode.active)DisplayMode.leave() } }
    Overlay.ImmersiveView { anchors.fill: parent; z: 6; window: window; controls: driving }
    PluginViewHost { anchors.fill: parent; z: 2.5; window: window; standardPlayerHeight: player.height; chrome:[host,systemControls,sidebar] }
    // Nothing starts with the keyboard: the search field only takes it when
    // it is clicked or asked for.
    Component.onCompleted: {
        contentItem.forceActiveFocus()
        console.info("main window constructed")
    }

    // Over everything while the plugins start.
    StartupSplash { anchors.fill: parent; z: 1000 }
    // A press outside the field that has the keyboard, or another view, takes it.
    FocusRelease { key: [Navigation.active, Navigation.screen, Navigation.routeId, Navigation.screen === "folders" ? "" : Navigation.routeKey].join(" ") }
}
