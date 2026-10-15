import QtQuick
import app.chilpa
import app.chilpa.controls

// Menu items, shortcuts and command lines share this action dispatcher.
QtObject {
    required property var controls
    required property var commandBar
    required property var sheet
    required property var settings
    readonly property var sidebarTurns: ({ open: "collapsed", collapsed: "closed", closed: "open" })
    function volume(by) {
        if (!Player.volumeLocked)
            Player.chooseVolume(Math.max(0, Math.min(100, Player.volume + by)))
    }

    function run(action, args) {
        args = args || ({})
        if (action.startsWith("list.")) {
            if (Hotkeys.searchActive) {
                if (action === "list.play") Hotkeys.searchConfirmed()
                else if (action === "list.down" || action === "list.up") Hotkeys.searchStepped(action === "list.down" ? 1 : -1)
                return
            }
            const list = controls.focusedList()
            if (list) list.viAction(action.slice(5))
            return
        }
        switch (action) {
        case "commands.open": commandBar.show("", controls.window.activeFocusItem); break
        case "commands.refresh": Hotkeys.refresh(); break
        case "search.next": Hotkeys.searchStepped(1); break
        case "search.previous": Hotkeys.searchStepped(-1); break
        case "playback.play": if (Player.paused) Player.toggle(); else if (!Player.playing) ListenList.command("play", "", ""); break
        case "playback.pause": if (Player.playing && !Player.paused) Player.toggle(); break
        case "playback.seek": Player.seek(Math.max(0, args.value + (args.relative ? Player.position : 0))); break
        case "playback.volume": if (!Player.volumeLocked) Player.chooseVolume(Math.max(0, Math.min(100, args.value + (args.relative ? Player.volume : 0)))); break
        case "playback.sleep": Controls.sleepIn(args.argument === "off" ? 0 : Number(args.argument)); break
        case "playback.loop": Controls.refresh(); break
        case "settings.set": settings.set(args.key, args.argument); break
        case "nav.go":
            if (args.argument === "album") { if (Player.albumId !== 0) Navigation.go("album", Player.albumId) }
            else if (args.argument === "artist") Navigation.openCredit(Player.artist, -1)
            else if (args.argument === "settings") Navigation.openSection("")
            else Navigation.go("library", 0)
            break
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
        case "nav.search":
            if (args.argument !== undefined) Hotkeys.searchCommand(args.argument, args.target || "all")
            else if (JSON.parse(Hotkeys.snapshot || "{}").keymap === "vi") commandBar.show("search ", controls.window.activeFocusItem)
            else Hotkeys.searchAsked()
            break
        case "nav.settings": Navigation.openSection(""); break
        case "nav.newTab": Navigation.open("library", 0); break
        case "nav.closeTab": Navigation.close(Navigation.active); break
        case "nav.nextTab": Navigation.step(1); break
        case "nav.previousTab": Navigation.step(-1); break
        case "queue.next": ListenList.command("next", "", ""); Navigation.go("listenlist", 0); break
        case "queue.previous": ListenList.command("previous", "", ""); Navigation.go("listenlist", 0); break
        case "nav.quit": controls.requestQuit(); break
        case "ui.sidebar": SettingsModel.sidebarMode = sidebarTurns[SettingsModel.sidebarMode] || "open"; break
        case "ui.queue":
            if (args.argument === "clear") ListenList.command("clear", args.target || "", "")
            else if ((args.argument || "").startsWith("save ")) ListenList.command("saveNamed", args.target || "", args.argument.slice(5).trim().replace(/^([\'"])(.*)\1$/, "$2"))
            else SettingsModel.queueOpen = args.argument !== undefined ? true : !SettingsModel.queueOpen
            break
        case "ui.lyrics": SettingsModel.lyricsOpen = !SettingsModel.lyricsOpen; break
        case "ui.escape": if (SettingsModel.queueOpen) SettingsModel.queueOpen = false; break
        case "ui.showShortcuts": sheet.open(); break
        case "ui.openLink": Hotkeys.linkAsked(); break
        case "ui.miniPlayer": controls.miniAsked(); break
        case "ui.focusMode": controls.immersiveAsked(); break
        case "focus.seekForward": Player.nudge(5); break
        case "focus.seekBack": Player.nudge(-5); break
        case "focus.seekForwardLong": Player.nudge(10); break
        case "focus.seekBackLong": Player.nudge(-10); break
        }
    }

}
