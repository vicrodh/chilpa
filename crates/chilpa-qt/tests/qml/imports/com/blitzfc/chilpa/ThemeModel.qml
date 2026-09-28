pragma Singleton
import QtQuick

QtObject {
    property int rev: 0
    property bool dark: true
    property bool systemDark: true
    function color(key, rev) { return "#808080" }
    function shadowColor(key, rev) { return "#404040" }
    function placeholder(index, rev) { return "#404040" }
}
