pragma Singleton
import QtQuick
QtObject {
    property int revision: 0
    property var state: ({ panels: [{ id: "radio", name: "Radio", icon: "broadcast" }], source: "radio", name: "Radio", crumbs: [], entries: [], searchable: true, loading: false, more: false, error: "" })
    property var asked: []
    readonly property string snapshot: JSON.stringify(state)
    function art(plugin, image, rev) { return "" }
    function artistKey(id, path) { return "" }
    function card(id, rev) { return "{}" }
    function albumLibraries(rev) { return JSON.stringify(state.albumLibraries || []) }
    function trackLibraries(rev) { return JSON.stringify(state.trackLibraries || []) }
    function refresh() {}
    function page(key, rev) { return JSON.stringify({ music: false }) }
    function command(action, id, text) { asked = asked.concat([[action, id, text]]) }
    function show(change) { state = Object.assign({}, state, change); revision += 1 }
}
