import QtQuick
QtObject {
    property string lastAction: ""
    property int lastRow: -1
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
