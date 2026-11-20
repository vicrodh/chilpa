import QtQuick
import app.chilpa.sources
ListModel {
    property int limit: 0; property string kind; property string key; property string order; property string group; property string filter; property int contentRev; property int albums: kind === "recentlyAdded" ? 2 : kind === "artists" ? 3 : 0; property int columns: 0; property int maxRows: 0
    property string filterContext
    property int rev: 0
    property bool stableItems: false
    property int snapshotRev: 0
    property bool publishRowsOnRevision: true
    onRevChanged: { if (publishRowsOnRevision) for (let row = 0; row < count; row++) setProperty(row, "revision", rev) }
    property int expanded: -1
    property int fresh: -1
    property bool mixed: false
    property bool ready: true
    property string error: ""
    property int retries: 0
    function retry() { retries++; error=""; ready=false }
    function albumId(index, revision) { return kind === "ids" ? JSON.parse(key)[index] : kind === "source" ? -(index + 1) : index + 1 }
    function title(index, revision) { return "Record " + index }
    property var explicitRows: []
    property var aiRows: []
    function aiGenerated(index, revision) { return aiRows.includes(index) }
    function explicitContent(index, revision) { return explicitRows.includes(index) }
    function subtitle(index, revision) { return "Artist" }
    function year(index, revision) { return "2026" }
    property string artSource: ""
    function art(index, revision, visible) {
        if (arguments.length !== 3 || typeof visible !== "boolean") throw new Error("AlbumGridModel.art requires index, revision and visibility")
        return artSource
    }
    // An artist's card, as Artist Scene reads it: the library and one source.
    function artist(index, revision) {
        const name = "Artist " + index
        return JSON.stringify({title: name, sources: [{local: true, name: "Chilpa", key: name, icon: ""},
            {plugin: "fixture", name: "Fixture", key: "fixture matching-artist-v3:" + name, icon: ""}]})
    }
    function prepareOpen(index) {}
    function select(index, modifiers) {}
    function fillSource() {
        if (kind === "ids") {
            clear()
            const ids = JSON.parse(key || "[]")
            albums = ids.length
            const shown = maxRows === 1 ? Math.min(columns, albums) : albums
            for (let index = 0; index < shown; index++) append({first:index,span:1,group:"",revision:rev})
            return
        }
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
