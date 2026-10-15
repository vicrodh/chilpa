pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property string presetName: ""
    property bool available: false
    property bool active: false
    property var actions: []
    function refresh() {}
    function command(action, input) { actions = actions.concat([{ action: action, input: JSON.parse(input) }]) }
}
