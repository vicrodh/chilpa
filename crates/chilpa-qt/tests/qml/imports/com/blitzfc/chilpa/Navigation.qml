pragma Singleton
import QtQuick
QtObject {
    property string page: ""
    property int selectedId: 0
    property string routeKey: "radio"
    property var went: []
    function goKey(screen, key) { went = [screen, key] }
    function go(target, id) { page = target; selectedId = id }
    function creditStops(credit) { return "" }
    function openCredit(credit, index) {}
}
