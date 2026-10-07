pragma Singleton
import QtQuick
QtObject {
    property bool active: false
    property string destination: ""
    property string snapshot: "{}"
    property bool picking: false
    property string selected: ""
    property bool disconnected: false
    function picker(open) { picking = open }
    function refresh() {}
    function connectDevice(id) { selected = id }
    function disconnectDevice() { disconnected = true }
}
