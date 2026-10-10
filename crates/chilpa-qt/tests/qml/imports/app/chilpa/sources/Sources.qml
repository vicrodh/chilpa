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
    function artColors(id, rev) { return "[]" }
    function albumLibraries(rev) { return JSON.stringify(state.albumLibraries || []) }
    function trackLibraries(rev) { return JSON.stringify(state.trackLibraries || []) }
    function homeLibraries(rev) { return JSON.stringify(state.homeLibraries || []) }
    function refresh() {}
    // Pages by key, as a test sets them.
    property var pages: ({})
    function page(key, rev) { return pages[key] || JSON.stringify({ music: false }) }
    property var networks: ({})
    function network(name, rev) { return networks[name] || JSON.stringify({ loading: false }) }
    function command(action, id, text) { asked = asked.concat([[action, id, text]]) }
    function show(change) { state = Object.assign({}, state, change); revision += 1 }
}
