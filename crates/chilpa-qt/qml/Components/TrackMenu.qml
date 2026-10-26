import QtQuick
import app.chilpa
import app.chilpa.devices
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Context rules come from the facade; QML only decorates and dispatches.
CascadingMenu {
    id: menu
    property TrackListModel model
    property int row: -1

    function show(at, left, top) {
        row = at
        const icons = {
            play: Icons.playerPlayFilled, queue: Lucide.listEnd,
            next: Lucide.listStart, later: Lucide.listPlus,
            pick: Icons.playlistAdd, album: Icons.disc, artist: SourceGlyphs.musicArtist,
            folder: Icons.folder, sync: Icons.deviceMobile,
            remove: Icons.trash, dequeue: Icons.trash,
            up: Icons.chevronUp, down: Icons.chevronDown
        }
        entries = JSON.parse(model.menuJson(row)).map(function(entry) {
            entry.glyph = icons[(entry.act || "").split(":")[0]] || ""
            if (["queue", "next", "later"].includes(entry.act)) entry.family = Lucide.font
            if ((entry.act || "").split(":")[0] === "artist") entry.family = SourceGlyphs.font
            entry.filled = entry.act === "play"
            return entry
        })
        if (entries.length === 0) return
        x = left
        y = top
        open()
    }

    onActionChosen: (entry) => {
        const act = entry.act
        if (act.startsWith("buffer:") || act.startsWith("listen:")) {
            model.listAction(act.startsWith("listen:") ? act.slice("listen:".length) : act, row, -1)
            ListenList.refresh()
            if (act === "buffer:new") Navigation.go("listenlist", 0)
        } else if (act === "play")
            model.playChosen(row)
        else if (act === "next")
            model.playNext(row)
        else if (act === "later")
            model.playLater(row, "")
        else if (act === "queue")
            model.queue(row)
        else if (act === "pick") {
            picker.trackIdsJson = model.chosenTrackIdsJson(row)
            picker.show(x, y)
        } else if (act === "sync") {
            DeviceSync.useTracks(model.chosenTrackIdsJson(row))
            Navigation.go("deviceSync", 0)
        } else if (act === "dequeue")
            Player.removeQueued(row)
        else if (act === "remove")
            model.removeChosen(row)
        else if (act === "up" || act === "down")
            model.moveInPlaylist(row, act === "up" ? -1 : 1)
        else
            model.go(row, act)
    }

    PlaylistPicker { id: picker; parent: menu.parent }
}
