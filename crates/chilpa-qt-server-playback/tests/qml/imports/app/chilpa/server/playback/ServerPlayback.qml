pragma Singleton
import QtQuick

// The test double of the ServerPlayback bridge: it answers with what a test
// sets, keeps each change the way the facade does and writes it down.
QtObject {
    property var state: ({ name: "Music server", kind: "subsonic", reports: true, counts_apart: true, report: true, count: true, mark: 50 })
    property var calls: []
    function settings(id) { return JSON.stringify(Object.assign({ id: id }, state)) }
    function change(id, name, value) {
        calls = calls.concat([[id, name, value]])
        const next = Object.assign({}, state)
        next[name === "reportPlayback" ? "report" : name === "countPlays" ? "count" : "mark"] = name === "playMark" ? Number(value) : value === "true"
        state = next
        return true
    }
}
