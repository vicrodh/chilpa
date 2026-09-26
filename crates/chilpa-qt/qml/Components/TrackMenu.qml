import QtQuick
import com.blitzfc.chilpa
import "../Primitives"
import "../Theme"

// What a track row offers on a right click: play it, play it next, queue it
// or add it to a playlist -- or the whole selection when it is part of one --
// and go to its record, its artist or its folder; on a playlist's page, take
// it out. Only what the row has somewhere to go is listed.
Menu {
    id: menu

    property TrackListModel model
    property int row: -1
    property var actions: []

    function show(at, left, top) {
        row = at
        // Several selected: the menu is for all of them, and has nowhere
        // one of them goes to (design/04 9).
        const many = model.selectedCount > 1
        // A queued track (design/04 9, "Ítem de queue"). Stop-after waits
        // for the player to have it; clear-after can be built from removing
        // along the queue's order and is not built yet.
        const queued = [
            { text: I18n.tr("Play now", I18n.rev), act: "play", glyph: Icons.playerPlayFilled, filled: true },
            { text: I18n.tr("Remove from queue", I18n.rev), act: "dequeue" },
            { separator: true },
            { text: I18n.tr("Add to playlist…", I18n.rev), act: "pick", submenu: true },
            { separator: true },
            { text: I18n.tr("Go to album", I18n.rev), act: "album" },
            { text: I18n.tr("Go to artist", I18n.rev), act: "artist" }
        ]
        const all = model.kind === "queue" ? queued : [
            { text: I18n.trCount("{} track", "{} tracks", model.selectedCount, I18n.rev), note: true },
            { text: I18n.tr("Play now", I18n.rev), act: "play", glyph: Icons.playerPlayFilled, filled: true },
            { text: I18n.tr("Play next", I18n.rev), act: "next" },
            { text: I18n.tr("Add to queue", I18n.rev), act: "queue" },
            { text: I18n.tr("Add to playlist…", I18n.rev), act: "pick", submenu: true },
            { text: I18n.tr("Remove from playlist", I18n.rev), act: "unlist" },
            { text: I18n.tr("Move up", I18n.rev), act: "up", hint: "Alt ↑" },
            { text: I18n.tr("Move down", I18n.rev), act: "down", hint: "Alt ↓" },
            { separator: true },
            { text: I18n.tr("Go to album", I18n.rev), act: "album" },
            { text: I18n.tr("Go to artist", I18n.rev), act: "artist" },
            { text: I18n.tr("Show in folder", I18n.rev), act: "folder" }
        ]
        const shown = all.filter(e => e.note ? many
            : e.act === "unlist" ? model.kind === "playlist" && !many
            : e.act === "up" ? model.kind === "playlist" && !many && row > 0
            : e.act === "down" ? model.kind === "playlist" && !many && row < model.count - 1
            : !e.act || ["play", "next", "queue", "pick", "dequeue"].includes(e.act) || (!many && model.canGo(row, e.act)))
        // A separator with nothing after it is a line for nothing.
        if (shown[shown.length - 1].separator)
            shown.pop()
        actions = shown
        entries = shown
        x = left
        y = top
        open()
    }

    onChosen: (index) => {
        const act = actions[index].act
        if (act === "play")
            model.playChosen(row)
        else if (act === "next")
            model.playNext(row)
        else if (act === "queue")
            model.queue(row)
        else if (act === "pick")
            picker.show(x, y)
        else if (act === "dequeue")
            Player.removeQueued(row)
        else if (act === "unlist")
            model.removeFromPlaylist(row)
        else if (act === "up" || act === "down")
            model.moveInPlaylist(row, act === "up" ? -1 : 1)
        else
            model.go(row, act)
    }
    onSubmenu: (index, top) => picker.show(menu.x + menu.width - Metrics.gap4, top)

    PlaylistPicker {
        id: picker

        // Beside the track menu, not inside it: that one is closed by then.
        parent: menu.parent
        onPicked: (playlist) => menu.model.addToPlaylist(menu.row, playlist)
    }
}
