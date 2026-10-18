import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: actions
    property var sidebar
    property var selected: ({})
    signal createPlaylist()
    // An order of the list: the one in use carries the caret of its
    // direction and choosing it again turns it around, as a SortButton
    // does. `own` is true for an order that comes most first, null for one
    // with no direction.
    function order(value, text, own) {
        const on = actions.sidebar.order === value
        if (!on || own === null) return {text: text, action: "sort", value: value, checked: on}
        const down = own === actions.sidebar.ascending
        return {text: text, action: "sort", value: value, glyph: down ? Icons.chevronDown : Icons.chevronUp, accent: true}
    }
    function head(left, top) {
        menu.entries = [
            {text: I18n.tr("New playlist", I18n.rev), action: "create", glyph: Icons.plus},
            {text: I18n.tr("New folder", I18n.rev), action: "folder", glyph: Icons.folderPlus},
            {text: I18n.tr("Import playlist", I18n.rev), action: "import", glyph: Lucide.importGlyph, family: Lucide.font},
            {separator: true},
            {text: I18n.tr("Group playlists by source", I18n.rev), action: "group", checked: actions.sidebar.grouped},
            {separator: true},
            order("name", I18n.tr("Name", I18n.rev), false),
            order("recent", I18n.tr("Recent", I18n.rev), true),
            order("tracks", I18n.tr("Number of tracks", I18n.rev), true),
            order("custom", I18n.tr("Custom", I18n.rev), null),
            {separator: true},
            {text: I18n.tr("Refresh", I18n.rev), action: "refresh", glyph: Icons.refresh},
            {text: I18n.tr("Manage playlists", I18n.rev), action: "manage", glyph: Lucide.libraryBig, family: Lucide.font}]
        menu.x = left; menu.y = top; menu.open() }
    function row(value, left, top) {
        selected = value
        const folder = value.rowKind === 1
        if (folder && value.nativeRef && value.editable === false) return
        let entries = folder ? [
            {text: I18n.tr("Edit folder", I18n.rev), action: "editFolder"},
            {text: I18n.tr("Delete folder…", I18n.rev), action: "deleteFolder"}
        ] : [
            {text: I18n.tr("Open playlist", I18n.rev), action: "open"},
            {text: I18n.tr("Play now", I18n.rev), action: "play"},
            {text: I18n.tr("Shuffle", I18n.rev), action: "shuffle"},
            {text: I18n.tr("Add to queue", I18n.rev), action: "queue"},
            {text: I18n.tr("Hide from sidebar", I18n.rev), action: "hide"}]
        if (!folder && value.local) entries.push(
            {text: I18n.tr("Edit playlist", I18n.rev), action: "edit"},
            {text: I18n.tr("Delete…", I18n.rev), action: "deletePlaylist"})
        entries = entries.concat([
            {separator: true},
            {text: I18n.tr("Move up", I18n.rev), action: "move", value: -1},
            {text: I18n.tr("Move down", I18n.rev), action: "move", value: 1}])
        if (!folder) entries.push({separator:true},{text:I18n.tr("Move to folder…",I18n.rev),action:"moveFolder"})
        if (folder && value.nativeRef) entries = [{text:I18n.tr("Edit folder",I18n.rev),action:"editNativeFolder"}]
        menu.entries = entries
        menu.x = left; menu.y = top; menu.open()
    }
    function move(by) {
        const folder = selected.rowKind === 1
        const catalog = JSON.parse(Playlists.catalogJson(""))
        const items = (folder ? catalog.folders : catalog.playlists.filter(p => (p.folder_id || 0) === selected.folderId)).slice()
        items.sort((a,b) => (folder ? a.position - b.position : a.custom_position - b.custom_position))
        const ids = items.map(item => item.id)
        const from = ids.indexOf(folder ? selected.folderId : selected.playlistId), to = from + by
        if (from < 0 || to < 0 || to >= ids.length) return
        const moved = ids.splice(from, 1)[0]; ids.splice(to, 0, moved)
        if (folder) Playlists.orderFolders(JSON.stringify(ids)); else Playlists.setCustomOrder(JSON.stringify(ids))
        sidebar.setOrder("custom")
    }
    Menu {
        id: menu; objectName: "playlistsMenu"
        heightLimit: actions.sidebar.height
        onChosen: index => {
            const item = entries[index], row = actions.selected
            switch (item.action) {
            case "create": actions.createPlaylist(); break
            case "import": Navigation.go("playlistImport", 0); break
            case "folder": folderDialog.show(0, "", "accent"); break
            case "group": actions.sidebar.setGrouped(!actions.sidebar.grouped); break
            case "sort": if (item.value === actions.sidebar.order && item.value !== "custom") actions.sidebar.reverseOrder(); else actions.sidebar.setOrder(item.value); break
            case "refresh": Playlists.refresh(); break
            case "manage": Navigation.go("playlistManager", 0); break
            case "play": Playlists.play(row.playlistId, false); break
            case "shuffle": Playlists.play(row.playlistId, true); break
            case "queue": Playlists.queue(row.playlistId); break
            case "deletePlaylist":
                menu.entries = [{text: I18n.trArgs("Delete “{}”?", [row.name], I18n.rev), note: true},
                    {text: I18n.tr("Delete", I18n.rev), action: "confirmPlaylist"}, {text: I18n.tr("Cancel", I18n.rev)}]
                menu.open(); break
            case "confirmPlaylist": Playlists.remove(row.playlistId); break
            case "open": actions.sidebar.openPlaylist(row.playlistId); break
            case "edit": if (row.local) editDialog.show(row.playlistId); else actions.sidebar.openPlaylist(row.playlistId); break
            case "hide": Playlists.setVisible(row.playlistId, false); break
            case "moveFolder": folderDialog.move(row); break
            case "editNativeFolder": folderDialog.showNative(row); break
            case "file": Playlists.moveToFolder(row.playlistId, item.value); break
            case "move": actions.move(item.value); break
            case "editFolder": folderDialog.show(row.folderId, row.name, row.folderColor); break
            case "deleteFolder":
                menu.entries = [{text: I18n.tr("Playlists will be kept at the root.", I18n.rev), note: true},
                    {text: I18n.tr("Delete folder", I18n.rev), action: "confirmDelete"},
                    {text: I18n.tr("Cancel", I18n.rev)}]
                menu.open(); break
            case "confirmDelete": Playlists.removeFolder(row.folderId); break
            }
        }
    }
    PlaylistFolderDialog { id: folderDialog }
    PlaylistEditDialog { id: editDialog }
}
