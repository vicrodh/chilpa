import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Confirmations
import app.chilpa
import app.chilpa.controls
import app.chilpa.devices
import app.chilpa.sources
import app.chilpa.kit
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
            remove: Icons.trash, dequeue: Icons.trash, server_delete: Icons.trash,
            up: Icons.chevronUp, down: Icons.chevronDown, info: Icons.infoCircle
        }
        entries = JSON.parse(model.menuJson(row)).map(function(entry) {
            entry.glyph = icons[(entry.act || "").split(":")[0]] || ""
            if (["queue", "next", "later"].includes(entry.act)) entry.family = Lucide.font
            if ((entry.act || "").split(":")[0] === "artist") entry.family = SourceGlyphs.font
            entry.filled = entry.act === "play"
            return entry
        })
        const chosen = model.chosenTrackIdsJson(row)
        if (JSON.parse(Sources.offline("info", "tracks", chosen)).count > 0) {
            if (entries.length > 0) entries = entries.concat([{separator:true}])
            entries = entries.concat([{act:"offline",text:I18n.tr("Make available offline…", I18n.rev)}])
        }
        if (entries.length === 0) return
        x = left
        y = top
        open()
    }

    onActionChosen: (entry) => {
        const act = entry.act
        if (act === "offline") {
            Sources.offlineRequested("tracks", model.chosenTrackIdsJson(row))
            return
        }
        if (act === "remove") {
            removal.show(model,row)
            return
        }
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
        else if (act === "server_delete") {
            const ids = JSON.parse(model.chosenTrackIdsJson(row))
            if (ids.length === 1) Sources.serverMediaRequested("track", ids[0])
        } else if (act === "info")
            NowAbout.trackInfoRequested(entry.path, entry.title)
        else if (act === "up" || act === "down")
            model.moveInPlaylist(row, act === "up" ? -1 : 1)
        else
            model.go(row, act)
    }

    PlaylistPicker { id: picker; parent: menu.parent }
    Confirmations.ModelActionConfirmation { id: removal }
}
