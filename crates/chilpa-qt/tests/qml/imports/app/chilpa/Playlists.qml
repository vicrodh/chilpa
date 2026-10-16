pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property var recentRows: []
    function recent(allSources, limit, revision) { return JSON.stringify(recentRows.filter(row => allSources || row.mixed).slice(0, limit)) }
    property var rows: []
    property var created: []
    property string added: ""
    property bool fail: false
    property var sourceChoices: []
    property var remoteWrite: []
    property var remoteState: ({})
    function sourcesJson(ids) { return JSON.stringify(sourceChoices) }
    function writeSource(source, id, name, description, tracks, request) {
        remoteWrite = [source, id, name, description, tracks]
        remoteState = {request_id: request, pending: true}
        return !fail
    }
    function writeState(source) { return JSON.stringify(remoteState) }
    function orderFolders(ids) { created = JSON.parse(ids) }
    function setCustomOrder(ids) { created = JSON.parse(ids) }
    function moveToFolder(id, folder) { created = [id, folder]; return !fail }
    function setVisible(id, visible) { created = [id, visible] }
    property int refreshes: 0
    function refresh() { refreshes++; rev++ }

    function pickerJson(ids, query) { return JSON.stringify(rows.filter(r => r.playlist.name.toLowerCase().includes(query.toLowerCase()))) }
    property int lastTarget: 0
    property bool repeated: false
    function addTrackIds(id, ids, repeats) { added = ids; repeated = repeats; if (!fail) lastTarget = id; return JSON.stringify({ added: 0, exact: 1, similar: 1, rejected: fail }) }
    function createFull(name, description, artwork, tags, folder, ids) { created = [name, description, artwork, tags, folder, ids]; return fail ? 0 : 7 }
    function chooseArtwork() { return "" }
    function folderId(id, revision) { return 4 }
    function name(id, revision) { return "Stored playlist" }
    function description(id, revision) { return "Stored description" }
    function tags(id, revision) { return "night, jazz" }
    function artwork(id, revision) { return "cover.png" }
    function update(id, name, description, artwork, tags, folder) { created = [name, description, artwork, tags, folder, id]; return !fail }
    function createFolder(name, color) { created = [name, color]; return fail ? 0 : 4 }
    function updateFolder(id, name, color) { created = [name, color, id]; return !fail }
    property var catalog: ({folders: [], playlists: []})
    function catalogJson(query) { return JSON.stringify(catalog) }
}
