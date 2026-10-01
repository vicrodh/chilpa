pragma Singleton
import QtQuick
QtObject {
    property string page: ""
    property int selectedId: 0
    property string routeKey: "radio"
    function go(target, id) { page = target; selectedId = id }
    function creditStops(credit) { return "" }
    function openCredit(credit, index) {}
}
