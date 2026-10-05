import QtQuick
import com.blitzfc.chilpa
import "../Primitives"
import "../Theme"

// Every collection shares placements in the player and the extra queues.
CascadingMenu {
    id: menu

    property TrackListModel model
    property string laterLabel
    property bool canRemove: false
    property int groupRow: -1
    property string activeLabel

    signal remove()

    function show(left, top) {
        groupRow = -1
        activeLabel = laterLabel
        showAt(left, top)
    }

    function showGroup(row, label, left, top) {
        groupRow = row
        activeLabel = label
        showAt(left, top)
    }

    function showAt(left, top) {
        const icons = {queue: Lucide.listEnd, next: Lucide.listStart, later: Lucide.listPlus}
        entries = JSON.parse(model.menuJson(-1)).map(entry => Object.assign({}, entry, {glyph: icons[entry.act] || "", family: Lucide.font})).concat(canRemove ? [
            { separator: true },
            { text: I18n.tr("Delete playlist", I18n.rev), act: "remove" }
        ] : [])
        x = left
        y = top
        open()
    }

    onActionChosen: entry => {
        const action = entry.act
        if (groupRow >= 0) {
            model.groupAction(groupRow, action, activeLabel)
            if (action.startsWith("buffer:")) { ListenList.refresh(); if (action === "buffer:new") Navigation.go("listenlist", 0) }
        }
        else if (action.startsWith("buffer:")) {
            model.listAction(action, -1, -1)
            ListenList.refresh()
            if (action === "buffer:new") Navigation.go("listenlist", 0)
        } else if (action === "queue")
            model.queueAll()
        else if (action === "next")
            model.playNextAll()
        else if (action === "later")
            model.playLaterAll(activeLabel)
        else if (action === "remove")
            menu.remove()
    }
}
