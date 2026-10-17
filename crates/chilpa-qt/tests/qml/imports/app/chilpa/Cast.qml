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
    function connectDevice(id) { selected = id; active = true }
    function disconnectDevice() { disconnected = true; active = false }
}
