import QtQuick
ListModel {
    property int limit: 0; property string kind; property string key; property string order; property string group; property string filter; property int contentRev; property int albums: kind === "recentlyAdded" ? 2 : 0; property int columns: 0; property int maxRows: 0
    property int rev: 0
    property int expanded: -1
    property int fresh: -1
    property bool mixed: false
    function albumId(index, revision) { return kind === "source" ? -(index + 1) : index + 1 }
    function title(index, revision) { return "Record " + index }
    function subtitle(index, revision) { return "Artist" }
    function year(index, revision) { return "2026" }
    function art(index, revision) { return "" }
    function prepareOpen(index) {}
    function select(index, modifiers) {}
}
