pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property bool open: false
    property bool discogsQuota: false
    property int quotaTaken: 0
    function takeDiscogsQuota() { if (!discogsQuota) return false; discogsQuota = false; quotaTaken++; return true }
    // What the facade's editor says, as the screen reads it.
    property var view: ({ state: "closed", album_id: 0, dirty: false, error: "" })
    property var opened: []
    property int saved: 0
    property int closed: 0
    property bool choosingBeetsRuntime: false
    property var engineCalls: []
    property var runtimeCalls: []
    property var changesChosen: []
    property int lookupCancels: 0
    property int probes: 0
    property int searches: 0
    property int applied: 0
    property var loadedHits: []
    function document(rev) { return JSON.stringify(view) }
    function openAlbum(album, track) { opened = opened.concat([album]) }
    function save() { ++saved }
    function close() { ++closed }
    function setEngine(engine) { engineCalls = engineCalls.concat([engine]); updateLookup({engine:engine,provider:"musicbrainz"}); return true }
    function setProvider(provider) { updateLookup({provider:provider}); return true }
    function setBeetsRuntime(runtime) { runtimeCalls = runtimeCalls.concat([runtime]); updateLookup({runtime:runtime}); return true }
    function chooseBeetsRuntime() {}
    function probeBeets() { probes++; updateLookup({searching:true,probing:true}); return true }
    function search() { searches++; return true }
    function cancelLookup() { lookupCancels++; updateLookup({searching:false,loading:false,probing:false,beets_busy:false}); }
    function load(hit) { loadedHits = loadedHits.concat([hit]); return true }
    function selectDisc(disc) { updateLookup({selected_disc:disc}); return true }
    function apply() { applied++; return 2 }
    function chooseChange(key, selected) {
        changesChosen = changesChosen.concat([{key:key,selected:selected}])
        updateLookup({changes:(view.lookup.changes || []).map(change => change.key === key ? Object.assign({},change,{selected:selected}) : change)})
        return true
    }
    function updateLookup(values) { show(Object.assign({},view,{lookup:Object.assign({},view.lookup || {},values)})) }
    // Where a save was asked to go, in order.
    property var modes: []
    function setMode(mode) { modes = modes.concat([mode]); return true }
    function revert() {}
    function show(next) { view = next; ++rev }
}
