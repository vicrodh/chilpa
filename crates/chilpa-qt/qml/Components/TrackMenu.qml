import QtQuick
import com.blitzfc.chilpa
import "../Primitives"
import "../Theme"

// What a track row offers on a right click: play it, play it next or queue
// it -- or the whole selection when it is part of one -- and go to its
// record, its artist or its folder. Only what the row has somewhere to go is
// listed.
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
        const all = [
            { text: I18n.trCount("{} track", "{} tracks", model.selectedCount, I18n.rev), note: true },
            { text: I18n.tr("Play", I18n.rev), act: "play" },
            { text: I18n.tr("Play next", I18n.rev), act: "next" },
            { text: I18n.tr("Add to queue", I18n.rev), act: "queue" },
            { separator: true },
            { text: I18n.tr("Go to album", I18n.rev), act: "album" },
            { text: I18n.tr("Go to artist", I18n.rev), act: "artist" },
            { text: I18n.tr("Show in folder", I18n.rev), act: "folder" }
        ]
        const shown = all.filter(e => e.note ? many
            : !e.act || e.act === "play" || e.act === "next" || e.act === "queue" || (!many && model.canGo(row, e.act)))
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
        else
            model.go(row, act)
    }
}
