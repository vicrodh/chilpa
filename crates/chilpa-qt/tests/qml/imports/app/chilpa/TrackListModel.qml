import QtQuick
ListModel {
    property int limit: 0
    property bool ready: true
    property string kind: ""
    property double albumId: 0
    property string key: ""
    property int filterRev: 0
    property int albumPage: 0
    property string playingPath: ""
    property string order: "custom"
    property bool ascending: true
    property string summary: "3 tracks"
    property bool playlistEditable: false
    function selectedRow() { return -1 }
    // The longest texts of each column, as the bridge gives them, when a
    // test asks for them; otherwise nothing, and the proportions stand.
    property bool measures: false
    function widest(rev) {
        if (!measures) return ""
        const rows = [], longest = key => rows.map(key).sort((a, b) => String(b).length - String(a).length).slice(0, 3)
        for (let i = 0; i < count; ++i) if (get(i).rowKind === 0) rows.push(get(i))
        return JSON.stringify({title: rows.map(row => [row.title, row.composer || ""]).sort((a, b) => (b[0] + b[1]).length - (a[0] + a[1]).length).slice(0, 3),
            artist: longest(row => row.artist), album: longest(row => row.album)})
    }
    function listAction(action, row, to) { lastAction = action; lastRow = row }
    property string lastAction: ""
    property int lastRow: -1
    function canGo(row, kind) { return true }
    function creditOf(row) { return get(row).artist || "" }
    function select(row, mode) { lastAction = "select"; lastRow = row }
    function isTrack(row) { return row >= 0 && row < count && get(row).rowKind === 0 }
    function queue(row) { lastAction = "queue"; lastRow = row }
    function favoriteChosen(row, favorite) { lastAction = favorite ? "favorite" : "unfavorite"; lastRow = row }
    function play(row) { lastAction = "play"; lastRow = row }
    function go(row, kind) { lastAction = kind; lastRow = row }
    function moveInPlaylistTo(row, to) { lastAction = "move"; lastRow = row; movedTo = to }
    function moveInPlaylist(row, by) { lastAction = by < 0 ? "up" : "down"; lastRow = row; ++steps }
    property int steps: 0
    property int selectedCount: 0
    property int movedTo: -1
    function queueAll() { lastAction = "queue" }
    function playNextAll() { lastAction = "next" }
    function playLaterAll(label) { lastAction = "later" }
    function groupAction(row, action, label) { lastAction = action; lastRow = row }
    function menuJson(row) {
        if (row < 0) return JSON.stringify([
            {act:"queue",text:"Add to queue"}, {act:"next",text:"Play next"}, {act:"later",text:"Play later"},
            {separator:true}, {text:"Extra queues",submenu:true,children:[
                {act:"buffer:new",text:"Open in new queue"}, {act:"buffer:add:2",text:"Add to queue Other"}, {act:"buffer:replace:2",text:"Replace queue Other"}
            ]}
        ])
        return JSON.stringify([
            { act: "play", text: "Play now" },
            { act: "queue", text: "Add to queue" },
            { act: "next", text: "Play next" },
            { act: "later", text: "Play later" },
            { separator: true },
            { act: "pick", text: "Add to playlist…" },
            { act: "sync", text: "Sync to device…" }
        ])
    }
    function chosenTrackIdsJson(row) { return "[42,43]" }
    function playLater(row, label) { lastAction = "later"; lastRow = row }
    function playAll(shuffled) { lastAction = shuffled ? "shuffle" : "playAll" }
    function playingRow() {
        for (let row = 0; row < count; ++row)
            if (get(row).playing) return row
        return -1
    }
    function selectAll() {}
    function clearSelection() {}
    function selectedTrackIdsJson() { return "[42,43]" }
    function letters(rev) { return "[]" }
    // A key that ends in "|tracks" (a page's tracks shelf) holds eight
    // rows and Most played thirty, cut to `limit`.
    onKeyChanged: fill()
    onKindChanged: fill()
    onLimitChanged: fill()
    Component.onCompleted: fill()
    property string filledKey: ""
    function fill() {
        const wanted = key.endsWith("|tracks") ? 8 : kind === "mostPlayed" ? 30 : 0
        const count = limit > 0 ? Math.min(limit, wanted) : wanted
        if (filledKey !== key) { clear(); filledKey = key }
        while (this.count > count) remove(this.count - 1)
        for (let i = this.count; i < count; i++)
            append({ trackId: i + 1, rowKind: 0, number: i + 1, title: "Track " + i, artist: "Band", album: "Record", sourceName: "", duration: "3:00", plays: kind === "mostPlayed" ? 30 - i : 0,
                quality: "FLAC", tier: 2, available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
    }
}
