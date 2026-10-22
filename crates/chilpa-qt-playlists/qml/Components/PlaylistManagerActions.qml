import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

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
    readonly property bool editable: row.local === true || (row.entryId < 0 && row.writable === true)
    readonly property bool removable: row.local === true || row.entryId < 0

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
        switch (entry.act) {
        case "open": manager.open(row.index); break
        case "play": Playlists.play(row.entryId, false); break
        case "edit":
            if (folder) editFolder(row.entryId, row.name, row.colorName)
            else if (row.local) editPlaylist(row.entryId)
            else editSource(row.entryId)
            break
        case "visible": manager.setVisible(row.entryId, !row.visibleInSidebar); break
        case "folder": manager.moveToFolder(row.entryId, entry.id); break
        case "remove":
            // Asked again: what is the listener's is deleted, and one only
            // followed is taken from the library.
            entries = [{text: folder || editable ? I18n.trArgs("Delete “{}”?", [row.name], I18n.rev) : row.name, note: true},
                       {text: folder || editable ? I18n.tr("Delete", I18n.rev) : I18n.tr("Remove from library", I18n.rev), act: "confirm"},
                       {text: I18n.tr("Cancel", I18n.rev)}]
            open()
            break
        case "confirm":
            if (folder) manager.removeFolder(row.entryId)
            else Playlists.remove(row.entryId)
            break
        }
    }
}
