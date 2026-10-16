import QtQuick
ListModel {
    property string path: ""
    property string query: ""
    property bool content: false
    property bool tree: false
    property bool grouped: false
    property bool hasCd: false
    property bool loading: false
    property bool inLibrary: false
    property int rev: 0
    property int contentRev: 0
    property int folders: 0
    property int tracks: 0
    property int discPicks: 0
    signal folderOpened()
    function pickDisc() { discPicks += 1 }
    function pick() {}
    function turn(path) {}
    function reveal(path) {}
    function act(path, action) {}
    function playAll() {}
    function playCd() {}
}
