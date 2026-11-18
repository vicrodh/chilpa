pragma Singleton
import QtQuick
QtObject {
    property bool active: false
    property string destination: ""
    property string snapshot: "{}"
    property bool picking: false
    property string selected: ""
    property bool disconnected: false
    property int refreshes: 0
    property var nextActive: null
    function picker(open) { picking = open }
    function refresh() { ++refreshes; if (nextActive !== null) { active = nextActive; nextActive = null } }
    property var acts: []
    function act(action, protocol, id) { acts = acts.concat([[action, protocol, id]]); if (action === "connect") { selected = id; active = true } else if (action === "local") { disconnected = true; active = false } }
}
