pragma Singleton
import QtQuick

// The link bridge as the tests see it: it keeps what it was asked.
QtObject {
    property bool open: false
    property bool resolving: false
    property string error: ""
    property bool playlist: false
    property string choices: "[]"
    property string submitted: ""
    property int chosen: -1
    property int importers: 0
    signal opened(string screen, real id, string key)
    function show() { error = ""; playlist = false; choices = "[]"; open = true }
    function close() { open = false; resolving = false }
    function submit(url) { submitted = url; resolving = true }
    function openImporter() { importers += 1; open = false; return true }
    function choose(index) { chosen = index; open = false }
}
