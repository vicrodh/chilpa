pragma Singleton
import QtQuick
QtObject {
    property int revision: 0
    property var state: ({ panels: [{ id: "radio", name: "Radio", icon: "broadcast" }], source: "radio", name: "Radio", crumbs: [], entries: [], searchable: true, loading: false, more: false, error: "" })
    property var asked: []
    readonly property string snapshot: JSON.stringify(state)
    function refresh() {}
    function command(action, id, text) { asked = asked.concat([[action, id, text]]) }
    function show(change) { state = Object.assign({}, state, change); revision += 1 }
}
