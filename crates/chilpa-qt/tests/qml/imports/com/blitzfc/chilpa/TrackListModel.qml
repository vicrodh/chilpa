import QtQuick
ListModel {
    property string kind: ""
    property double albumId: 0
    property string key: ""
    property int filterRev: 0
    property string playingPath: ""
    property string summary: "3 tracks"
    function listAction(action, row, to) { lastAction = action; lastRow = row }
    property string lastAction: ""
    property int lastRow: -1
    function canGo(row, kind) { return true }
    function select(row, mode) { lastAction = "select"; lastRow = row }
    function play(row) { lastAction = "play"; lastRow = row }
    function go(row, kind) { lastAction = kind; lastRow = row }
    function moveInPlaylistTo(row, to) { lastAction = "move"; lastRow = row; movedTo = to }
    function moveInPlaylist(row, by) { lastAction = by < 0 ? "up" : "down"; lastRow = row; ++steps }
    property int steps: 0
    property int selectedCount: 0
    property int movedTo: -1
    function menuJson(row) {
        return JSON.stringify([
            { act: "play", text: "Play now" },
            { act: "queue", text: "Add to queue" },
            { act: "next", text: "Play next" },
            { act: "later", text: "Play later" },
            { separator: true },
            { act: "pick", text: "Add to playlist…" }
        ])
    }
    function chosenTrackIdsJson(row) { return "[42,43]" }
    function playLater(row, label) { lastAction = "later"; lastRow = row }
}
