import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// I reconcile model snapshots into a bounded image pool. Model resets
// never tear down a cover that is already moving through the fan.
Item {
    id: deck
    property var ahead: null
    property var behind: null
    property string currentPath: ""
    property url currentArt
    property bool live: true
    property bool shaders: true
    property real artSize: Stage.flowArtMin
    property bool mounted: false
    readonly property int capacity: Stage.flowNeighbours * 2 + 3 // derived: both sides, the center and two spare images

    function schedule() { Qt.callLater(reconcile) }
    onCurrentPathChanged: schedule()
    onCurrentArtChanged: schedule()
    onLiveChanged: if (live) schedule()
    Component.onCompleted: schedule()
    Connections {
        target: deck.ahead
        function onModelReset() { deck.schedule() }
        function onDataChanged() { deck.schedule() }
        function onCountChanged() { deck.schedule() }
    }
    Connections {
        target: deck.behind
        function onModelReset() { deck.schedule() }
        function onDataChanged() { deck.schedule() }
        function onCountChanged() { deck.schedule() }
    }
    function record(model, index, offset, history) {
        if (!model || index < 0 || index >= model.count) return null
        // ListModel is also useful in the gallery and deterministic tests.
        const row = typeof model.get === "function" ? model.get(index) : null
        const at = row ? null : model.index(index, 0)
        const id = row ? row.trackId : model.data(at, Stage.flowTrackRole)
        const art = row ? row.art : model.data(at, Stage.flowArtRole)
        return {identity: "track:" + id, art: art || "", row: index, offset: offset, history: history}
    }
    function snapshot() {
        const rows = []
        const playing = ahead ? ahead.playingRow() : -1
        const cursor = Math.max(0, playing)
        const center = playing >= 0 ? record(ahead, cursor, 0, false) : null
        rows.push(center || {identity: "path:" + currentPath, art: currentArt, row: -1, offset: 0, history: false})
        if (rows[0].art.toString() === "" && currentArt.toString() !== "") rows[0].art = currentArt
        for (let i = 1; i <= Stage.flowNeighbours; ++i) {
            const next = record(ahead, cursor + i, i, false)
            if (next) rows.push(next)
            const previous = cursor >= i ? record(ahead, cursor - i, -i, false) : record(behind, i - cursor - 1, -i, true)
            if (previous) rows.push(previous)
        }
        return rows
    }
    function covers() {
        const result = []
        for (let i = 0; i < pool.count; ++i) result.push(pool.itemAt(i))
        return result
    }
    function reconcile() {
        if (!live || !pool.count) return
        const rows = snapshot(), cards = covers(), used = []
        const snap = !mounted || Motion.reduced
        let shift = 1
        const center = cards.find(c => c.occupied && c.identity === rows[0].identity)
        if (center) shift = center.destination
        const matches = rows.map(row => {
            let match = null
            for (const card of cards) {
                if (!card.occupied || card.identity !== row.identity || used.indexOf(card) >= 0) continue
                if (!match || Math.abs(card.destination - row.offset - shift) < Math.abs(match.destination - row.offset - shift)) match = card
            }
            if (match) used.push(match)
            return match
        })
        for (let i = 0; i < rows.length; ++i) {
            const row = rows[i]
            let card = matches[i]
            if (!card) {
                card = cards.filter(c => used.indexOf(c) < 0).sort((a, b) => Math.abs(b.offset) - Math.abs(a.offset))[0]
                used.push(card)
                card.occupied = false
                card.identity = row.identity
                card.artwork = row.art
                card.offset = row.offset + (snap ? 0 : shift)
            } else if (row.art.toString() !== "") card.artwork = row.art
            card.row = row.row
            card.history = row.history
            card.occupied = true
            if (card.destination !== row.offset || (card.offset !== row.offset && !matches[i]) || snap) card.move(row.offset, snap)
        }
        for (const card of cards) if (used.indexOf(card) < 0) card.occupied = false
        mounted = true
    }
    Repeater {
        id: pool
        model: deck.capacity
        CoverFlowCard {
            objectName: "flowCover"
            artSize: deck.artSize
            live: deck.live
            shaders: deck.shaders
            onNavigate: history ? deck.behind.play(row) : deck.ahead.play(row)
        }
    }
}
