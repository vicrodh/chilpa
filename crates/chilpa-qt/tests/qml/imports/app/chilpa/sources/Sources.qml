pragma Singleton
import QtQuick
import app.chilpa
QtObject {
    property int revision: 0
    property var state: ({ panels: [{ id: "radio", name: "Radio", icon: "broadcast" }], source: "radio", name: "Radio", crumbs: [], entries: [], searchable: true, loading: false, more: false, error: "" })
    property var asked: []
    readonly property string snapshot: JSON.stringify(state)
    // A cover a test says arrived, by its address.
    property var arrived: ({})
    function art(plugin, image, rev) { return arrived[image] || "" }
    function artistKey(id, path) { return "" }
    function artistLink(id, name) { return JSON.stringify(AlbumActions.artistDestination.key ? AlbumActions.artistDestination : {target:"artist",key:name}) }
    function card(id, rev) { return JSON.stringify((state.cards || {})[id] || {}) }
    function favoritePending(plugin, reference, rev) {
        const action = (state.native_actions || {})[plugin] || {}
        return action.pending === true && action.favorite?.ref === reference
    }
    function artColors(id, rev) { return JSON.stringify((state.artColors || {})[id] || []) }
    function albumLibraries(rev) { return JSON.stringify(state.albumLibraries || []) }
    function trackLibraries(rev) { return JSON.stringify(state.trackLibraries || []) }
    function homeLibraries(rev) { return JSON.stringify(state.homeLibraries || []) }
    property int refreshCount: 0
    function refresh() { refreshCount++ }
    // Pages by key, as a test sets them.
    property var pages: ({})
    function page(key, rev) { return pages[key] || JSON.stringify({ music: false }) }
    property var networks: ({})
    function network(name, rev) { return networks[name] || JSON.stringify({ loading: false }) }
    property var sceneDocument: ({state:"idle",artists:[]})
    function scene(rev) { return JSON.stringify(sceneDocument) }
    function sceneOpen(name, includeSources) { asked = asked.concat([["sceneOpen", name, includeSources]]) }
    function sceneMore() { asked = asked.concat([["sceneMore"]]) }
    function sceneRetry() { asked = asked.concat([["sceneRetry"]]) }
    function sceneClose() { asked = asked.concat([["sceneClose"]]) }
    function referenceCard(plugin, reference) { return -7 }
    function command(action, id, text) { asked = asked.concat([[action, id, text]]) }
    function show(change) { state = Object.assign({}, state, change); revision += 1 }
}
