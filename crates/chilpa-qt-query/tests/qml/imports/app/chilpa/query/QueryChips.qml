pragma Singleton
import QtQuick

// The chips bridge as the tests see it: two chips, the second kept out, and
// each edit answered with what was asked.
QtObject {
    property string chipsOf: "{\"any\":false,\"chips\":[{\"label\":\"Genre: Jazz\",\"text\":\"genre:Jazz\",\"negated\":false},{\"label\":\"Label: Verve\",\"text\":\"-label:Verve\",\"negated\":true}]}"
    function read(query) { return query === "" ? "{\"any\":false,\"chips\":[]}" : chipsOf }
    function without(query, at) { return "without:" + at }
    function flipped(query, at) { return "flipped:" + at }
    function joined(query, any) { lastJoin = [query, any]; return focusTesting ? joinedText : "joined:" + any }
    function excluding(query, column, value) { return "excluding:" + column + ":" + value }
    property bool focusTesting: false
    property string joinedText: ""
    property var lastJoin: []
    property var focusOf: ({})
    property string editedText: ""
    property var lastEdit: ({})
    property string lastQuery: ""
    property var facets: ({})
    property int liveFacets: 0
    function focusRows(query) {
        return JSON.stringify(focusOf[query] || { any: false, advanced: false, text: query, rows: [] })
    }
    function edit(query, change) { lastQuery = query; lastEdit = JSON.parse(change); return editedText }
    // Refine's reading and edits (the facade's JSON), by query text.
    property var refineOf: ({})
    property var lastRefineEdit: ({})
    property string refinedText: ""
    function refine(query) { return JSON.stringify(refineOf[query] || { mode: "all", rows: [], complex: null }) }
    function refineEdit(query, change) { lastQuery = query; lastRefineEdit = JSON.parse(change); return refinedText }
    property bool canUndo: false
    property string undoneText: ""
    function undo() { return undoneText }
    property var observed: []
    function observe(text) { observed = observed.concat([text]) }
}
