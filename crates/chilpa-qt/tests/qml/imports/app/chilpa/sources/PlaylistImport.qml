pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property var asked: []
    // Whether a running plugin imports playlists.
    property bool importer: true
    function available() { return importer }
    function refresh() {}
    function command(action, payload) { asked = asked.concat([{action: action, payload: JSON.parse(payload)}]) }
    function show(value) { snapshot = JSON.stringify(value) }
}
