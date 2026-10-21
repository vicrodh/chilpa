pragma Singleton
import QtQuick
QtObject {
    property string summary: ""; property string order: "artist"; property bool busy: false
    property int listeningRev: 0
    property string albumOrder: "artist"; property int rev: 0; property int contentRev: 0; property int remoteRev: 0; property int remoteArtRev: 0
    property var queries: ["", "", "", "", "", "", ""]
    property var sourceKeys: []; property var sourceLabels: []; property var sourceCounts: []; property var sourceIcons: []
    property var excludedSources: []; property var qualities: []; property var formats: []
    property bool extraFiltered: false; property bool groupVersions: false
    property var chosen: []
    function clearColumn(column) { chosen.push([column, ""]) }
    function clear() { queryText = ""; extraFiltered = false }
    function searchColumn(column, text) {}
    property var asked: []
    function toggleFilter(kind, key) { asked = asked.concat([[kind, key]]) }
    function resetExtra() {}
    function groupAlbums(on) { groupVersions = on }
    property bool groupSources: true
    function groupCopies(on) { groupSources = on }
    property string queryText: ""
    function applyQuery(text) { queryText = text; return "" }
}
