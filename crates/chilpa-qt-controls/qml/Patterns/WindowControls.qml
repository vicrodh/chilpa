import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"

// What drives the player besides its own buttons, held once by the window:
// every key nothing else took is asked about here and its action run, the
// timer and the A-B loop are kept current for whoever shows them, and what
// the desktop asks of the window is done. And the window is put back where
// it was left, and says where it is once it settles.
Item {
    id: controls

    // The window whose keys these are.
    required property var window
    // The mini player, when the window has one: its key opens and closes
    // it, and the desktop's asks for the window go to it while it is up.
    property bool miniOpen: false
    signal miniAsked()
    signal presentMini()
    signal quitMini()
    // The immersive view over the window: while it is open the keys
    // answer in its scope, which is the window's plus its own.
    property bool immersiveOpen: false
    signal immersiveAsked()


    // A key typed into a field is that field's.
    function typing() {
        const item = window.activeFocusItem
        return item instanceof TextInput || item instanceof TextEdit
    }

    TrackFocus { id: trackFocus; window: controls.window; immersiveOpen: controls.immersiveOpen }
    function focusedList() { return trackFocus.focusedList() }
    function restoreFocus(previous) { trackFocus.restoreFocus(previous) }
    function pressed(event) {
        for (let item = window.activeFocusItem; item; item = item.parent) {
            if (item.blocksWindowShortcuts === true && !JSON.parse(Hotkeys.snapshot || "{}").recording)
                return
        }
        if (event.key === Qt.Key_Escape && typing() && JSON.parse(Hotkeys.snapshot || "{}").keymap === "vi") {
            const list = focusedList()
            if (list) { list.forceActiveFocus(); event.accepted = true; return }
        }
        const action = Hotkeys.pressedIn(controls.immersiveOpen ? "immersive" : "window", event.key, event.modifiers, event.text, typing())
        if (action === "")
            return
        event.accepted = true
        if (action !== "-")
            for (let n = 0; n < Math.max(1, Hotkeys.repeat || 1); n++) run(action)
    }

    WindowActions { id: actions; controls: controls; commandBar: commandBar; sheet: sheet; settings: commandSettings }
    function run(action, args) { actions.run(action, args) }

    // The window's own item gets every key nothing inside it took.
    property bool listening: false
    function listen() {
        if (listening || !window)
            return
        listening = true
        window.contentItem.Keys.pressed.connect(pressed)
    }

    NavigationPointer { window: controls.window }
    // The desktop asked Chilpa to quit: the close that follows is not one
    // to hide from.
    property bool quitting: false
    // A window knows where it is only where the system tells it.
    readonly property bool knowsPlace: !Qt.platform.pluginName.startsWith("wayland")

    function frame() {
        return window.visibility === Window.Windowed ? "floating"
             : window.visibility === Window.Maximized ? "maximized" : "away"
    }
    function keepPlace() {
        settle.stop()
        WindowPlace.keep(frame(), window.width, window.height, window.x, window.y, knowsPlace)
    }
    // A place kept from a screen that is no longer there is not gone back to.
    function onScreen(left, top) {
        return Qt.application.screens.some(s => left >= s.virtualX && left < s.virtualX + s.width
                                                && top >= s.virtualY && top < s.virtualY + s.height)
    }
    // Its size and whether it is maximized are the window's own bindings;
    // its place is put here, before it shows, and a size kept from a larger
    // screen is brought down to this one.
    function restorePlace() {
        if (!window) return
        const screen = window.screen
        if (screen && window.width > screen.desktopAvailableWidth)
            window.width = Math.max(window.minimumWidth, screen.desktopAvailableWidth)
        if (screen && window.height > screen.desktopAvailableHeight)
            window.height = Math.max(window.minimumHeight, screen.desktopAvailableHeight)
        if (knowsPlace && WindowPlace.placed && onScreen(WindowPlace.left, WindowPlace.top)) {
            window.x = WindowPlace.left
            window.y = WindowPlace.top
        }
    }

    onWindowChanged: listen()
    Component.onCompleted: {
        restorePlace()
        listen()
        Controls.refresh()
        Hotkeys.refresh()
    }

    // Quitting asks first, as QBZ's does, unless "Don't ask again" was
    // chosen: the window's Quit, Ctrl+Q, the tray's Quit and a close that
    // does not hide to the tray. The desktop's media controls quit at once.
    function requestQuit() {
        if (controls.quitting || question.opened)
            return
        if (!Controls.confirmsQuit())
            return controls.finishQuit()
        if (!controls.window.visible || controls.window.visibility === Window.Minimized)
            controls.present()
        question.ask()
    }
    // A hidden window has nothing to close: it quits. Behind the mini player
    // it quits through it: a quit closes every window first, and the mini
    // player's refuses to close while it is open.
    function finishQuit() {
        controls.quitting = true
        if (controls.miniOpen)
            return controls.quitMini()
        return controls.window.visible ? controls.window.close() : Qt.quit()
    }

    function present() {
        controls.window.show()
        controls.window.raise()
        controls.window.requestActivate()
    }
    Connections {
        target: Player
        // The desktop's media controls, Chilpa launched again, or the tray:
        // 0 come to the front, 1 quit, 2 show if hidden and hide if in sight,
        // 3 quit after asking (the tray's Quit).
        function onWindowAsked(what) {
            if (what === 1)
                return controls.finishQuit()
            if (what === 3)
                return controls.requestQuit()
            if (controls.miniOpen)
                return controls.presentMini()
            if (what === 2 && controls.window.visible)
                return controls.window.hide()
            controls.present()
        }
        function onPathChanged() { Controls.refresh() }
        // A jump outside the loop, or its track ending, lets it go.
        function onPositionChanged() { if (Controls.loopState > 0) Controls.refresh() }
    }
    Connections {
        target: controls.window
        function onWidthChanged() { settle.restart() }
        function onHeightChanged() { settle.restart() }
        function onXChanged() { settle.restart() }
        function onYChanged() { settle.restart() }
        function onVisibilityChanged() { settle.restart() }
        // Closing keeps Chilpa in the tray when that is wanted, and there
        // is a tray; otherwise it is quitting, which asks first. A quit
        // already answered closes.
        function onClosing(close) {
            controls.keepPlace()
            if (controls.quitting)
                return
            if (!Controls.closesToTray() && !Controls.confirmsQuit()) {
                controls.quitting = true
                return
            }
            close.accepted = false
            if (Controls.closesToTray())
                return controls.window.hide()
            controls.requestQuit()
        }
    }
    // A resize is many frames; only the one it ends on is kept.
    Timer { id: settle; interval: Sizes.placeSettle; onTriggered: controls.keepPlace() }
    Connections {
        target: I18n
        function onRevChanged() { Hotkeys.refresh(); Controls.relabelTray(); Navigation.activate(Navigation.active); ListenList.refresh() }
    }
    // An action asked for without its key: a row of the app menu.
    Connections {
        target: Hotkeys
        function onAsked(action) { controls.run(action) }
    }
    Timer {
        interval: Sizes.second
        repeat: true
        running: Controls.sleepLeft !== ""
        onTriggered: Controls.refresh()
    }

    QuitQuestion {
        id: question
        onQuit: (stopAsking) => {
            if (stopAsking)
                Controls.stopConfirmingQuit()
            controls.finishQuit()
        }
    }

    SettingsPage { id: commandSettings }
    CommandBar {
        id: commandBar
        parent: controls.window ? controls.window.contentItem : controls
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.playerSpace + Metrics.playerBottom
        z: Metrics.tooltipLayer
        onDismissed: previous => controls.restoreFocus(previous)
        onExecute: command => controls.run(command.action, command)
    }

    ShortcutsSheet {
        id: sheet
        onCustomize: editor.open()
    }
    ShortcutsEditor { id: editor }
}
