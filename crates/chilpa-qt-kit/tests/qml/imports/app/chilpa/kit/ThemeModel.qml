pragma Singleton
import QtQuick

QtObject {
    property int rev: 0
    property bool dark: true
    property bool systemDark: true
    property bool ambient: false
    property var palette: ({})
    function color(key, rev) { return palette[key] || "#808080" }
    function shadowColor(key, rev) { return "#404040" }
    function placeholder(index, rev) { return "#404040" }
}
