pragma Singleton
import QtQuick
QtObject {
    property bool pending: false
    property string page: ""
    property int selectedId: 0
    property int routeId: 0
    property string routeKey: "radio"
    property string screen: "source"
    property string section: ""
    property int active: 0
    property int backCount: 0
    property int forwardCount: 0
    property int stepCount: 0
    property var went: []
    function goKey(screen, key) { went = [screen, key] }
    function go(target, id) { page = target; selectedId = id }
    function creditStops(credit) { return "" }
    function openCredit(credit, index) {}
    function openArtist(name) { went = ["artist", name] }
    function openSection(id) { section = id; screen = "settings" }
    function back() { backCount++ }
    function forward() { forwardCount++ }
    function step(by) { stepCount += by }
    function activate(index) { active = index }
}
