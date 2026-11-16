import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The menu of a folder or playlist in the manager, one for every card and
// row: open and play it, edit it, keep it in the sidebar or not, move it to
// a folder, and remove it once asked again. A source's playlist is edited
// and removed at its source: deleted when it is the listener's, no longer
// followed when it is someone else's.
Menu {
    id: actions
    objectName: "playlistActions"
    property var manager
    property var row: ({})
    signal editFolder(int id, string name, string colorName)
    signal editPlaylist(int id)
    signal editSource(int id)
    readonly property bool folder: row.rowKind === 1
    // The library's own, or a source's the listener may change there.
    readonly property bool editable: row.local === true || (row.entryId < 0 && row.writable === true) || (row.server === true && row.writable === true)
    // A server's own the account may change is deleted on the server.
    readonly property bool removable: row.local === true || row.entryId < 0 || (row.server === true && row.writable === true)

    function show(value, item, left, top) {
        row = value
        const base = folder ? [
            {text: I18n.tr("Open", I18n.rev), act: "open"},
            {text: I18n.tr("Edit", I18n.rev), act: "edit"},
            {separator: true},
            {text: I18n.tr("Remove…", I18n.rev), act: "remove"}
        ] : [
            {text: I18n.tr("Open", I18n.rev), act: "open"},
            {text: I18n.tr("Play now", I18n.rev), act: "play"},
            {text: I18n.tr("Edit", I18n.rev), act: "edit", when: editable},
            {text: row.visibleInSidebar ? I18n.tr("Hide from sidebar", I18n.rev) : I18n.tr("Show in sidebar", I18n.rev), act: "visible"},
            {separator: true},
            {text: I18n.tr("Move to root", I18n.rev), act: "folder", id: 0}
        ]
        if (!folder) {
            if (row.entryId > 0)
                base.push(...JSON.parse(Offline.menu("playlist", JSON.stringify([row.entryId]))))
            const ids = manager.folderIds(), names = manager.folderNames()
            for (let i = 0; i < ids.length; ++i)
                base.push({text: names[i], act: "folder", id: Number(ids[i])})
            if (removable)
                base.push({separator: true}, {text: I18n.tr("Remove…", I18n.rev), act: "remove"})
        }
        entries = base.filter(entry => entry.when === undefined || entry.when)
        const at = item.mapToItem(actions.parent, left, top)
        x = at.x
        y = at.y
        open()
    }

    onChosen: (at) => {
        const entry = entries[at]
        // Offline happens where the listener is; the rows show how it goes.
        if ((entry.act || "").startsWith("offline:")) return Offline.call(entry.act.split(":")[1], "playlist", JSON.stringify([row.entryId]))
        switch (entry.act) {
        case "open": manager.open(row.index); break
        case "play": Playlists.play(row.entryId, false); break
        case "edit":
            if (folder) editFolder(row.entryId, row.name, row.colorName)
            else if (row.local || row.server === true) editPlaylist(row.entryId)
            else editSource(row.entryId)
            break
        case "visible": manager.setVisible(row.entryId, !row.visibleInSidebar); break
        case "folder": manager.moveToFolder(row.entryId, entry.id); break
        case "remove":
            deletion.targetId = row.entryId
            deletion.folder = folder
            deletion.title = I18n.trArgs("Delete “{}”?", [row.name], I18n.rev)
            deletion.message = row.server === true && !folder
                ? I18n.trArgs("Delete “{}” on {}? It leaves the server too.", [row.name, row.sourceName], I18n.rev)
                : folder ? I18n.tr("Playlists in this folder move to the root.", I18n.rev)
                : editable ? I18n.tr("This permanently removes the playlist and its saved order.", I18n.rev)
                : I18n.tr("Remove this playlist from your library?", I18n.rev)
            deletion.open()
            break
        }
    }
    ConfirmationDialog {
        id: deletion
        objectName: "playlistDeletionConfirmation"
        property int targetId: 0
        property bool folder: false
        destructive: true
        confirmText: I18n.tr("Remove", I18n.rev)
        onConfirmed: folder ? actions.manager.removeFolder(targetId) : Playlists.remove(targetId)
    }

}
