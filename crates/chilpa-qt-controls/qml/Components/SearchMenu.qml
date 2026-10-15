import QtQuick
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// What a search result offers from its menu. The entries and what each
// does come from the facade; here they get their icons and are dispatched.
Menu {
    id: menu
    objectName: "searchResultMenu"

    property SearchModel search
    property int row: -1

    // An entry took the window somewhere else.
    signal left()

    function show(at, left, top) {
        row = at
        const icons = {
            play: Icons.playerPlayFilled, queue: Lucide.listEnd,
            next: Lucide.listStart, later: Lucide.listPlus,
            pick: Icons.playlistAdd, album: Icons.disc, artist: SourceGlyphs.musicArtist,
            folder: Icons.folder
        }
        entries = JSON.parse(search.menuJson(row)).map(function(entry) {
            const act = (entry.act || "").split(":")[0]
            const favorite = act === "mark" || act === "favorite" || act === "unfavorite"
            const marked = act === "unfavorite" || act === "mark" && !entry.on
            entry.glyph = favorite ? (marked ? Icons.heartFilled : Icons.heart) : icons[act] || ""
            if (act === "artist") entry.family = SourceGlyphs.font
            else if (["queue", "next", "later"].includes(act)) entry.family = Lucide.font
            entry.filled = act === "play" || marked
            return entry
        })
        if (entries.length === 0) { search.releaseMenu(); return }
        x = left
        y = top
        open()
    }

    onClosed: Qt.callLater(function() { if (!menu.visible && menu.search) menu.search.releaseMenu() })

    onChosen: (index) => {
        const entry = entries[index]
        if (entry.act === "mark") {
            Ratings.setFavorite(entry.kind, entry.key, entry.on)
        } else if (entry.act === "pick") {
            search.pickMenu()
        } else {
            if (!["play", "queue", "next", "later", "favorite", "unfavorite"].includes(entry.act))
                menu.left()
            search.menuAct(entry.act)
        }
    }

    SearchSelection { parent: menu.parent; search: menu.search }
}
