import QtQuick
// Stands in for the bridge: an active request with a key resolves at once.
QtObject {
    property string key: ""
    property bool header: false
    property bool active: false
    readonly property string source: active && key !== ""
        ? "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='8' height='" + (header ? 2 : 8) + "'/%3E" : ""
}
