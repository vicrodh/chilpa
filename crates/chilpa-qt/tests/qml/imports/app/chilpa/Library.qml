pragma Singleton
import QtQuick
QtObject {
    property string albumOrder: "artist"; property int rev: 0; property int contentRev: 0; property int remoteRev: 0; property int remoteArtRev: 0
    property string year: ""; property string genre: ""; property string label: ""; property string composer: ""; property string rating: ""; property string artist: ""; property string album: ""
    property var queries: ["", "", "", "", "", "", ""]
    property var sourceKeys: []; property var sourceLabels: []; property var sourceCounts: []; property var sourceIcons: []
    property var excludedSources: []; property var qualities: []; property var formats: []
    property bool extraFiltered: false; property bool groupVersions: false
    property var chosen: []
    function choose(column, key, label) { chosen.push([column, key]) }
    function searchColumn(column, text) {}
    function toggleFilter(kind, key) {}
    function resetExtra() {}
    function groupAlbums(on) { groupVersions = on }
}
