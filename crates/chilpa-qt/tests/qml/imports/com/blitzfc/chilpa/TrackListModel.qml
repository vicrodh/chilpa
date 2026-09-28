import QtQuick
ListModel {
    property string lastAction: ""
    property int lastRow: -1
    function canGo(row, kind) { return true }
    function select(row, mode) { lastAction = "select"; lastRow = row }
    function play(row) { lastAction = "play"; lastRow = row }
    function go(row, kind) { lastAction = kind; lastRow = row }
    function moveInPlaylistTo(row, to) { lastAction = "move"; lastRow = row }
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
