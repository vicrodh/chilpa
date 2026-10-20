import QtQuick
ListModel {
    id: model
    property string column: ""
    property int rev: 0
    property string query: ""
    property int total: Math.max(0, count - 1)
    function choose(row, modifiers) {}
    function index(row, column) { return row }
    function data(row, role) { return get(row).key }
    function reload() {
        clear()
        const rows = QueryChips.facets[column] || []
        for (const row of rows)
            if (row.all || row.label.toLowerCase().includes(query.toLowerCase())) append(Object.assign({ selected: false, albums: row.all ? "" : column === "album" ? "1" : row.meta }, row))
    }
    onColumnChanged: reload()
    onQueryChanged: reload()
    onRevChanged: reload()
    Component.onCompleted: { QueryChips.liveFacets++; reload() }
    Component.onDestruction: QueryChips.liveFacets--
}
