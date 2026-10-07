pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property string error: ""
    property url art: ""
    property bool loading: false
    property int revision: 0
    property string picked: ""
    property int closes: 0
    signal opened()
    function refresh() {}
    function pick(kind) { picked = kind }
    function close() { closes++; snapshot = "{}"; revision++ }
}
