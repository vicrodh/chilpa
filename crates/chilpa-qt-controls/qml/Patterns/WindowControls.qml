import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import com.blitzfc.chilpa.controls
import "../Theme"

// What drives the player besides its own buttons, held once by the window:
// every key nothing else took is asked about here and its action run, the
// timer and the A-B loop are kept current for whoever shows them, and what
// the desktop asks of the window is done.
Item {
    id: controls

    // The window whose keys these are.
    required property var window

    readonly property var sidebarTurns: ({ open: "collapsed", collapsed: "closed", closed: "open" })

    // A key typed into a field is that field's.
    function typing() {
        const item = window.activeFocusItem
        return item instanceof TextInput || item instanceof TextEdit
    }

    function pressed(event) {
        const action = Hotkeys.pressed(event.key, event.modifiers, event.text, typing())
        if (action === "")
            return
        event.accepted = true
        if (action !== "-")
            run(action)
    }

    function volume(by) {
        if (!Player.volumeLocked)
            Player.chooseVolume(Math.max(0, Math.min(100, Player.volume + by)))
    }

    function run(action) {
        switch (action) {
        case "playback.toggle": Player.toggle(); break
        case "playback.next": Player.next(); break
        case "playback.prev": Player.previous(); break
        case "playback.volumeUp": volume(5); break
        case "playback.volumeDown": volume(-5); break
        case "playback.mute": if (!Player.volumeLocked) Player.chooseMuted(!Player.muted); break
        case "playback.shuffle": Player.toggleShuffle(); break
        case "playback.repeat": Player.cycleRepeat(); break
        case "playback.favorite": Hotkeys.likePlaying(); Ratings.refresh(); break
        case "playback.seekForward": Player.nudge(5); break
        case "playback.seekBack": Player.nudge(-5); break
        case "nav.back": Navigation.back(); break
        case "nav.forward": Navigation.forward(); break
        case "nav.search": Hotkeys.searchAsked(); break
        case "nav.settings": Navigation.openSection(""); break
        case "nav.newTab": Navigation.open("library", 0); break
        case "nav.closeTab": Navigation.close(Navigation.active); break
        case "nav.quit": Qt.quit(); break
        case "ui.sidebar": SettingsModel.sidebarMode = sidebarTurns[SettingsModel.sidebarMode] || "open"; break
        case "ui.queue": SettingsModel.queueOpen = !SettingsModel.queueOpen; break
        case "ui.lyrics": SettingsModel.lyricsOpen = !SettingsModel.lyricsOpen; break
        case "ui.escape": if (SettingsModel.queueOpen) SettingsModel.queueOpen = false; break
        case "ui.showShortcuts": sheet.open(); break
        }
    }

    // The window's own item gets every key nothing inside it took.
    property bool listening: false
    function listen() {
        if (listening || !window)
            return
        listening = true
        window.contentItem.Keys.pressed.connect(pressed)
    }
    onWindowChanged: listen()
    Component.onCompleted: {
        listen()
        Controls.refresh()
        Hotkeys.refresh()
    }

    Connections {
        target: Player
        // The desktop's media controls, or Chilpa launched again.
        function onWindowAsked(quit) {
            if (quit)
                return controls.window.close()
            controls.window.show()
            controls.window.raise()
            controls.window.requestActivate()
        }
        function onPathChanged() { Controls.refresh() }
        // A jump outside the loop, or its track ending, lets it go.
        function onPositionChanged() { if (Controls.loopState > 0) Controls.refresh() }
    }
    Connections {
        target: I18n
        function onRevChanged() { Hotkeys.refresh() }
    }
    Timer {
        interval: Sizes.second
        repeat: true
        running: Controls.sleepLeft !== ""
        onTriggered: Controls.refresh()
    }

    ShortcutsSheet {
        id: sheet
        onCustomize: editor.open()
    }
    ShortcutsEditor { id: editor }
}
