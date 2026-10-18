pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property var calls: []
    function refresh() {}
    function command(action, input) { calls = calls.concat([[action, JSON.parse(input)]]) }
}
