import QtQuick
import com.blitzfc.chilpa
import "../Primitives"
import "../Theme"

// What a search result offers from its menu. The entries and what each
// does come from the facade; here they get their icons and are dispatched.
Menu {
    id: menu

    property SearchModel search
    property int row: -1

    // An entry took the window somewhere else.
    signal left()

    function show(at, left, top) {
        row = at
        const icons = {
            play: Icons.playerPlayFilled, queue: Lucide.listEnd,
            next: Lucide.listStart, later: Lucide.listPlus,
            pick: Icons.playlistAdd, album: Icons.disc, artist: Icons.microphone2,
            folder: Icons.folder
        }
        entries = JSON.parse(search.menuJson(row)).map(function(entry) {
            const act = (entry.act || "").split(":")[0]
            entry.glyph = act === "mark" ? (entry.on ? Icons.heart : Icons.heartFilled) : icons[act] || ""
            if (["queue", "next", "later"].includes(act)) entry.family = Lucide.font
            entry.filled = act === "play" || (act === "mark" && !entry.on)
            return entry
        })
        if (entries.length === 0) return
        x = left
        y = top
        open()
    }

    onChosen: (index) => {
        const entry = entries[index]
        if (entry.act === "mark") {
            Ratings.setFavorite(entry.kind, entry.key, entry.on)
        } else if (entry.act === "pick") {
            picker.trackIdsJson = search.trackIdsJson(row)
            picker.show(x, y)
        } else {
            if (!["play", "queue", "next", "later"].includes(entry.act))
                menu.left()
            search.act(row, entry.act)
        }
    }

    PlaylistPicker { id: picker; parent: menu.parent }
}
