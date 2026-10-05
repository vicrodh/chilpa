pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property var asked: []
    function refresh() {}
    function command(action, payload) { asked = asked.concat([{action: action, payload: JSON.parse(payload)}]) }
    function show(value) { snapshot = JSON.stringify(value) }
}
