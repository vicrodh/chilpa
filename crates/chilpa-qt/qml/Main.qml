import QtQuick
import app.chilpa
import app.chilpa.controls
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Driving
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Controls
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

    readonly property real sidebarWidth: sidebar.shown
    readonly property real contentLeft: Metrics.spaceWindow + sidebarWidth + Math.min(1, sidebarWidth / Metrics.sidebarCollapsed) * Metrics.spaceWindow
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

    ServerMediaDeletion {}
    Driving.TrackInfoRequests {}

    // The dynamic background, under everything (design/01 1.6).
    Backdrop.AppBackground {
        visible: !DisplayMode.active || !DisplayMode.provider || DisplayMode.provider.ambient !== false
        anchors.fill: parent
        plane: DisplayMode.active ? Qt.rect(0, 0, window.width, window.height) : Qt.rect(contentPlane.x, contentPlane.y, contentPlane.width, contentPlane.height)
    }

    // The screen scrolls under nothing but the player: it is cut at the
    // tab bar, which stays on top of it.
    Driving.ContentPlane {
        id: contentPlane
        moving: sidebar.moving || right.moving
        leftInset: window.contentLeft
        rightInset: Metrics.spaceWindow + right.width + Math.min(1, right.width / Math.max(1, right.settled)) * Metrics.spaceWindow
        topInset: plan.planeTop
        bottomInset: plan.contentBottom
        cornered: !DisplayMode.active && !Theme.ambient
        ScreenHost { id: host; anchors.fill: parent }
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

    Driving.RightPanelFrame {
        id: right
        z: 2
        open: !DisplayMode.active && (SettingsModel.queueOpen || SettingsModel.lyricsOpen) && !window.panelAside
        mediaContent: MediaListeningPanel { document: JSON.parse(Sources.snapshot || "{}"); onClosed: SettingsModel.lyricsOpen = false }
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
    Driving.PlayerPlan { id: plan; window: window; content: contentPlane; tall: player.implicitHeight; away: idle.away }

    PlayerIdle {
        id: idle
        target: window.contentItem
        holding: player.pointed
    }

    SelectionBar {
        z: 4
        visible: !DisplayMode.active && AlbumActions.selectedCount > 0
        x: DisplayMode.active ? (window.width - width) / 2 : contentPlane.x + (contentPlane.width - width) / 2 // derived: centered over the content
        y: window.height - height - Metrics.selectionBottom
    }

    Toast {
        z: 5
        x: DisplayMode.active ? (window.width - width) / 2 : contentPlane.x + (contentPlane.width - width) / 2 // derived: centered over the content
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
    Controls.QueueProgress { anchors.right: parent.right; anchors.rightMargin: Metrics.spaceWindow; y: plan.planeTop + Metrics.gap8; z: 7 }

    WindowGrips { anchors.fill: parent; window: window }
    DevHooks { window: window }

    Driving.WindowControls { id: driving; window: window; onVolumeKeyed: player.peekVolume() }
    Mini.MiniHost { window: window; controls: driving; beforeEnter:() => { if(DisplayMode.active)DisplayMode.leave() } }
    Overlay.ImmersiveView { anchors.fill: parent; z: 6; window: window; controls: driving }
    PluginViewHost { anchors.fill: parent; z: 2.5; window: window; standardPlayerHeight: player.height; chrome:[host,systemControls,sidebar] }
    // Nothing starts with the keyboard: the search field only takes it when
    // it is clicked or asked for.
    Component.onCompleted: {
        contentItem.forceActiveFocus()
        console.info("main window constructed")
    }

    // Over everything until what has to be read before a click is read.
    Driving.StartupSplash { anchors.fill: parent; z: 1000 }
    // A press outside the field that has the keyboard, or another view, takes it.
    FocusRelease { key: [Navigation.active, Navigation.screen, Navigation.routeId, Navigation.screen === "folders" ? "" : Navigation.routeKey].join(" ") }
}
