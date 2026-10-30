pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property string tab: "equipment"
    property bool shown: false
    property int reads: 0
    property var commands: []
    signal outputSelected()
    signal outputsListed()
    property string pathAnswer: "{}"
    property int pathReads: 0
    function signalPath() { pathReads++; return pathAnswer }
    function show(page) { tab = page; shown = true }
    function close() { shown = false; snapshot = "{}" }
    function command(action, argument) { commands = commands.concat([{action:action, argument:argument}]) }
    function refresh() { reads++ }
    function takeRequest() {}
}
