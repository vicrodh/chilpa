import QtQuick
ListModel {
    property string text: ""
    property string source: "all"
    property bool selectionPending: false
    property string selectionError: ""
    property string requestedAction: ""
    signal navigated(string kind, string target, int id, string token)
    signal playlistReady(string tracks)
    function menuJson(index) { return "[]" }
    function releaseNavigation(token) {}
    function releaseMenu() {}
    function menuAct(action) { requestedAction = action }
    function pickMenu() { selectionPending = true }
    function pollSelection() {}
    function cancelSelection() { selectionPending = false; selectionError = "" }
    function refresh() {}
    property string scope: "all"
    property string qualityFilter: ""
    property bool hideExplicit: false
    function act(index, action) {}
}
