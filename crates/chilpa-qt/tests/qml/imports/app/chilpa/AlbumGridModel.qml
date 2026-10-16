import QtQuick
import app.chilpa.sources
ListModel {
    property int limit: 0; property string kind; property string key; property string order; property string group; property string filter; property int contentRev; property int albums: kind === "recentlyAdded" ? 2 : 0; property int columns: 0; property int maxRows: 0
    property int rev: 0
    property int expanded: -1
    property int fresh: -1
    property bool mixed: false
    property bool ready: true
    function albumId(index, revision) { return kind === "source" ? -(index + 1) : index + 1 }
    function title(index, revision) { return "Record " + index }
    function subtitle(index, revision) { return "Artist" }
    function year(index, revision) { return "2026" }
    function art(index, revision) { return "" }
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
        const size = Math.max(1, columns)
        const shown = maxRows > 0 ? Math.min(albums, size * maxRows) : albums
        for (let first = 0; first < shown; first += size)
            append({ first: first, span: Math.min(size, shown - first), group: "" })
    }
    onKeyChanged: fillSource()
    onColumnsChanged: fillSource()
    onMaxRowsChanged: fillSource()
    onLimitChanged: fillSource()
    onContentRevChanged: fillSource()
}
