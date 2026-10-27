import QtQuick
import app.chilpa.sources
ListModel {
    property int limit: 0; property string kind; property string key; property string order; property string group; property string filter; property int contentRev; property int albums: kind === "recentlyAdded" ? 2 : kind === "artists" ? 3 : 0; property int columns: 0; property int maxRows: 0
    property int rev: 0
    property bool publishRowsOnRevision: true
    onRevChanged: { if (publishRowsOnRevision) for (let row = 0; row < count; row++) setProperty(row, "revision", rev) }
    property int expanded: -1
    property int fresh: -1
    property bool mixed: false
    property bool ready: true
    function albumId(index, revision) { return kind === "source" ? -(index + 1) : index + 1 }
    function title(index, revision) { return "Record " + index }
    property var explicitRows: []
    function explicitContent(index, revision) { return explicitRows.includes(index) }
    function subtitle(index, revision) { return "Artist" }
    function year(index, revision) { return "2026" }
    function art(index, revision) { return "" }
    // An artist's card, as Artist Scene reads it: the library and one source.
    function artist(index, revision) {
        const name = "Artist " + index
        return JSON.stringify({title: name, sources: [{local: true, name: "Chilpa", key: name, icon: ""},
            {plugin: "fixture", name: "Fixture", key: "fixture matching-artist-v3:" + name, icon: ""}]})
    }
    function prepareOpen(index) {}
    function select(index, modifiers) {}
    function fillSource() {
        if (kind !== "source" || !key.includes("native:")) return
        clear()
        const parts = key.split("|")
        const page = JSON.parse(Sources.page(parts[0], 0) || "{}")
        const section = (page.shelves || []).find(row => row.id === parts[1])
        const wanted = section ? (section.items || []).length : 18
        albums = limit > 0 ? Math.min(limit, wanted) : wanted
        // As the bridge does: a source shelf is ready once it has records, stopped loading or failed.
        const state = JSON.parse(Sources.page(key, 0) || "{}")
        ready = albums > 0 || state.loading !== true || !!state.error
        const size = maxRows === 1 ? 1 : Math.max(1, columns)
        const shown = maxRows > 0 ? Math.min(albums, columns * maxRows) : albums
        for (let first = 0; first < shown; first += size)
            append({ first: first, span: Math.min(size, shown - first), group: "", revision: rev })
    }
    onKeyChanged: fillSource()
    onColumnsChanged: fillSource()
    onMaxRowsChanged: fillSource()
    onLimitChanged: fillSource()
    onContentRevChanged: fillSource()
}
