pragma Singleton
import QtQuick
QtObject {
    property string answer: "{}"
    property var saved: []
    property var choices: []
    property string submitted: ""
    function prepare(input) { submitted = input; return answer }
    function complete(input) { return JSON.stringify(choices.filter(s => s.startsWith(input))) }
    function history() { return JSON.stringify(saved) }
}
