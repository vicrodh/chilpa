import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Every collection shares placements in the player and the extra queues.
CascadingMenu {
    id: menu

    property TrackListModel model
    property var collectionAction: null
    property string laterLabel
    property bool canRemove: false
    // A saved playlist offers to be written to a file.
    property bool canExport: false
    property int offlinePlaylistId: 0
    property int groupRow: -1
    property string activeLabel

    signal remove()
    // To a file: with its files, or as plain data (titles, artists, albums).
    signal exportList(bool plain)

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
        entries = JSON.parse(collectionAction ? QueueActivity.collectionMenu() : model.menuJson(-1)).map(entry => Object.assign({}, entry, {glyph: icons[entry.act] || "", family: Lucide.font})).concat(canRemove ? [
            { separator: true },
            { text: I18n.tr("Delete playlist", I18n.rev), act: "remove" }
        ] : []).concat(canExport && groupRow < 0 ? [{ separator: true }, { text: I18n.tr("Export…", I18n.rev), act: "export" }, { text: I18n.tr("Export plain data…", I18n.rev), act: "exportPlain" }] : [])
        const offline = groupRow < 0 && offlinePlaylistId > 0 ? JSON.parse(Offline.menu("playlist", JSON.stringify([offlinePlaylistId]))) : []
        if (offline.length > 0) entries = entries.concat([{separator:true}], offline)
        x = left
        y = top
        open()
    }

    onActionChosen: entry => {
        const action = entry.act
        if (collectionAction) {
            collectionAction(action, entry.text)
            return
        }
        if (action.startsWith("offline:")) {
            Offline.call(action.split(":")[1], "playlist", JSON.stringify([offlinePlaylistId]))
            return
        }
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
        else if (action === "export" || action === "exportPlain")
            menu.exportList(action === "exportPlain")
    }
}
