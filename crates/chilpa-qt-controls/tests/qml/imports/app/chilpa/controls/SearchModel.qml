import QtQuick
import app.chilpa
import app.chilpa.kit
ListModel {
    // The rows the results page reads, as the bridge gives them.
    property string rowsJsonText: "[]"
    property int count: JSON.parse(rowsJsonText).filter(row => row.kind > 0).length
    function rowsJson() { return rowsJsonText }
    function keepPreview(on) { SettingsModel.keep("navigation.searchPreview", on ? "true" : "false"); Prefs.changed() }
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
